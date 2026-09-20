//! Render manifests, caption generation, and subtitle serialization.
//! This module validates and describes the exact media artifact sent to renderers.
//! It keeps rendering deterministic by turning editorial decisions and transcripts into one contract.

use crate::domain::{Candidate, CaptionCue, EditManifest, EditorialDecision, TranscriptSegment};
use anyhow::{Result, bail, ensure};
use std::path::{Component, Path};

const MAX_CAPTION_WORDS: usize = 6;
const MAX_CAPTION_CHARACTERS: usize = 34;
const TRANSCRIPT_OVERLAP_TOLERANCE_MS: i64 = 1_200;
const HOOK_VISIBLE_MS: i64 = 3_000;

pub fn validate_safe_path(value: &str) -> Result<()> {
    ensure!(!value.is_empty(), "path must not be empty");
    ensure!(
        !value.contains(['\0', '\n', '\r']),
        "path contains control characters"
    );
    ensure!(
        !Path::new(value)
            .components()
            .any(|component| component == Component::ParentDir),
        "parent traversal is not allowed"
    );
    Ok(())
}

pub fn validate_object_key(value: &str) -> Result<()> {
    validate_safe_path(value)?;
    ensure!(
        !Path::new(value).is_absolute(),
        "object key must be relative"
    );
    Ok(())
}

pub fn validate_manifest(manifest: &EditManifest) -> Result<()> {
    ensure!(manifest.version == 1, "unsupported edit manifest version");
    ensure!(
        (5_000..=60_000).contains(&(manifest.source_end_ms - manifest.source_start_ms)),
        "render duration must be 5-60 seconds"
    );
    ensure!(
        manifest.width == 1080 && manifest.height == 1920,
        "master output must be 1080x1920"
    );
    ensure!((23..=60).contains(&manifest.fps), "fps must be 23-60");
    validate_safe_path(&manifest.input_path)?;
    validate_safe_path(&manifest.output_path)?;
    ensure!(
        Path::new(&manifest.output_path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4")),
        "render output must be mp4"
    );
    if !matches!(
        manifest.layout.as_str(),
        "fit_blur" | "tracked_crop" | "stacked" | "full_frame"
    ) {
        bail!("unsupported layout {}", manifest.layout);
    }
    if let Some(hook) = &manifest.hook_text {
        ensure!(hook.chars().count() <= 160, "hook text is too long");
        ensure!(
            !hook.contains(['\0', '\r']),
            "hook contains control characters"
        );
    }
    for cue in &manifest.captions {
        ensure!(
            cue.start_ms >= 0
                && cue.end_ms > cue.start_ms
                && cue.end_ms <= manifest.source_end_ms - manifest.source_start_ms,
            "caption cue is outside the clip"
        );
        ensure!(
            !cue.text.contains(['\0', '\r']),
            "caption contains control characters"
        );
    }
    Ok(())
}

pub fn build_manifest(
    candidate: &Candidate,
    decision: &EditorialDecision,
    input_path: &str,
    output_path: &str,
    transcript: &[TranscriptSegment],
) -> Result<EditManifest> {
    let start_ms = decision.start_ms.max(candidate.start_ms);
    let end_ms = decision.end_ms.min(candidate.end_ms);
    let captions = build_caption_cues(transcript, start_ms, end_ms);
    let manifest = EditManifest {
        version: 1,
        candidate_id: candidate.id.clone(),
        input_path: input_path.to_owned(),
        output_path: output_path.to_owned(),
        source_start_ms: start_ms,
        source_end_ms: end_ms,
        width: 1080,
        height: 1920,
        fps: 30,
        layout: decision
            .layout
            .clone()
            .unwrap_or_else(|| "fit_blur".to_owned()),
        hook_text: decision.hook_text.clone(),
        captions,
    };
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn srt_timestamp(ms: i64) -> String {
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1_000;
    let millis = ms % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

pub fn render_srt(cues: &[CaptionCue]) -> String {
    cues.iter()
        .enumerate()
        .map(|(index, cue)| {
            let text = cue.text.replace("-->", "→").replace('\n', " ");
            format!(
                "{}\n{} --> {}\n{}\n\n",
                index + 1,
                srt_timestamp(cue.start_ms),
                srt_timestamp(cue.end_ms),
                text
            )
        })
        .collect()
}

/// Builds a resolution-aware subtitle file instead of relying on libass's implicit
/// SRT canvas (normally 384x288). The implicit canvas was scaling an 18-point font
/// to roughly 120 output pixels and scaling the margins along with it.
pub fn render_ass(cues: &[CaptionCue], hook_text: Option<&str>, duration_ms: i64) -> String {
    let mut output = String::from(
        "[Script Info]\n\
         ScriptType: v4.00+\n\
         PlayResX: 1080\n\
         PlayResY: 1920\n\
         ScaledBorderAndShadow: yes\n\
         WrapStyle: 0\n\
         YCbCr Matrix: TV.709\n\n\
         [V4+ Styles]\n\
         Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
         Style: Caption,Arial,64,&H00FFFFFF,&H00FFFFFF,&H00000000,&H78000000,-1,0,0,0,100,100,0,0,1,5,1,2,110,110,430,1\n\
         Style: Hook,Arial,54,&H00FFFFFF,&H00FFFFFF,&H00000000,&H78000000,-1,0,0,0,100,100,0,0,3,10,0,8,120,120,150,1\n\n\
         [Events]\n\
         Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
    );

    if let Some(hook) = hook_text.map(str::trim).filter(|hook| !hook.is_empty()) {
        let hook_end = duration_ms.clamp(0, HOOK_VISIBLE_MS);
        if hook_end > 0 {
            output.push_str(&format!(
                "Dialogue: 1,{},{},Hook,,0,0,0,,{}\n",
                ass_timestamp(0),
                ass_timestamp(hook_end),
                escape_ass_text(hook)
            ));
        }
    }
    for cue in cues {
        output.push_str(&format!(
            "Dialogue: 0,{},{},Caption,,0,0,0,,{}\n",
            ass_timestamp(cue.start_ms),
            ass_timestamp(cue.end_ms),
            escape_ass_text(&cue.text)
        ));
    }
    output
}

#[derive(Debug)]
struct WorkingCaption {
    start_ms: i64,
    end_ms: i64,
    words: Vec<String>,
}

fn build_caption_cues(
    transcript: &[TranscriptSegment],
    clip_start_ms: i64,
    clip_end_ms: i64,
) -> Vec<CaptionCue> {
    let mut source = transcript
        .iter()
        .filter_map(|segment| {
            let start_ms = segment.start_ms.max(clip_start_ms);
            let end_ms = segment.end_ms.min(clip_end_ms);
            let words = segment
                .text
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            (end_ms > start_ms && !words.is_empty()).then_some(WorkingCaption {
                start_ms,
                end_ms,
                words,
            })
        })
        .collect::<Vec<_>>();
    source.sort_by_key(|segment| (segment.start_ms, segment.end_ms));

    let mut cleaned: Vec<WorkingCaption> = Vec::new();
    for mut segment in source {
        if let Some(previous) = cleaned.last() {
            let close_in_time = segment.start_ms
                < previous
                    .end_ms
                    .saturating_add(TRANSCRIPT_OVERLAP_TOLERANCE_MS);
            if close_in_time {
                let repeated = repeated_prefix_word_count(&previous.words, &segment.words);
                if repeated == segment.words.len() {
                    continue;
                }
                if repeated > 0 {
                    let duration = segment.end_ms - segment.start_ms;
                    segment.start_ms += duration * repeated as i64 / segment.words.len() as i64;
                    segment.words.drain(..repeated);
                }
            }
        }
        if segment.end_ms > segment.start_ms && !segment.words.is_empty() {
            cleaned.push(segment);
        }
    }

    // Sliding transcription windows can assign overlapping time ranges to adjacent
    // phrases. Split that shared range at its midpoint so libass never stacks two
    // active dialogue events on top of one another.
    for index in 1..cleaned.len() {
        let (before, after) = cleaned.split_at_mut(index);
        let previous = &mut before[index - 1];
        let current = &mut after[0];
        if current.start_ms < previous.end_ms {
            let overlap_start = current.start_ms.max(previous.start_ms);
            let overlap_end = current.end_ms.min(previous.end_ms);
            let boundary = overlap_start + (overlap_end - overlap_start).max(0) / 2;
            previous.end_ms = previous.end_ms.min(boundary);
            current.start_ms = current.start_ms.max(boundary);
        }
    }

    cleaned
        .into_iter()
        .filter(|segment| segment.end_ms > segment.start_ms)
        .flat_map(|segment| split_caption_segment(segment, clip_start_ms))
        .collect()
}

fn repeated_prefix_word_count(previous: &[String], current: &[String]) -> usize {
    let previous = previous
        .iter()
        .map(|word| word_key(word))
        .collect::<Vec<_>>();
    let current = current
        .iter()
        .map(|word| word_key(word))
        .collect::<Vec<_>>();
    if current.len() >= 2
        && previous
            .windows(current.len())
            .any(|window| window == current)
    {
        return current.len();
    }
    let maximum = previous.len().min(current.len());
    (1..=maximum)
        .rev()
        .find(|&count| previous[previous.len() - count..] == current[..count])
        .unwrap_or(0)
}

fn word_key(word: &str) -> String {
    word.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn split_caption_segment(segment: WorkingCaption, clip_start_ms: i64) -> Vec<CaptionCue> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    for word in segment.words {
        let current = groups.last_mut();
        let would_exceed = current.as_ref().is_some_and(|group| {
            let characters = group.iter().map(String::len).sum::<usize>() + group.len();
            group.len() >= MAX_CAPTION_WORDS
                || (!group.is_empty() && characters + word.len() > MAX_CAPTION_CHARACTERS)
        });
        if current.is_none() || would_exceed {
            groups.push(Vec::new());
        }
        groups.last_mut().expect("caption group").push(word);
    }

    let weights = groups
        .iter()
        .map(|group| group.iter().map(String::len).sum::<usize>() + group.len())
        .collect::<Vec<_>>();
    let total_weight = weights.iter().sum::<usize>().max(1) as i64;
    let duration = segment.end_ms - segment.start_ms;
    let mut consumed_weight = 0_i64;
    groups
        .into_iter()
        .zip(weights)
        .map(|(group, weight)| {
            let start_ms = segment.start_ms + duration * consumed_weight / total_weight;
            consumed_weight += weight as i64;
            let end_ms = segment.start_ms + duration * consumed_weight / total_weight;
            CaptionCue {
                start_ms: start_ms - clip_start_ms,
                end_ms: end_ms - clip_start_ms,
                text: group.join(" "),
            }
        })
        .filter(|cue| cue.end_ms > cue.start_ms)
        .collect()
}

fn ass_timestamp(ms: i64) -> String {
    let centiseconds = ms.max(0) / 10;
    let hours = centiseconds / 360_000;
    let minutes = (centiseconds % 360_000) / 6_000;
    let seconds = (centiseconds % 6_000) / 100;
    let fraction = centiseconds % 100;
    format!("{hours}:{minutes:02}:{seconds:02}.{fraction:02}")
}

fn escape_ass_text(text: &str) -> String {
    text.replace('\\', "＼")
        .replace('{', "（")
        .replace('}', "）")
        .replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> EditManifest {
        EditManifest {
            version: 1,
            candidate_id: "candidate".to_owned(),
            input_path: "/tmp/input.mp4".to_owned(),
            output_path: "/tmp/output.mp4".to_owned(),
            source_start_ms: 1_000,
            source_end_ms: 11_000,
            width: 1080,
            height: 1920,
            fps: 30,
            layout: "fit_blur".to_owned(),
            hook_text: None,
            captions: vec![CaptionCue {
                start_ms: 0,
                end_ms: 1_000,
                text: "hello".to_owned(),
            }],
        }
    }

    #[test]
    fn rejects_path_traversal_and_invalid_duration() {
        let mut value = manifest();
        value.output_path = "../escape.mp4".to_owned();
        assert!(validate_manifest(&value).is_err());
        let mut value = manifest();
        value.source_end_ms = 4_000;
        assert!(validate_manifest(&value).is_err());
    }

    #[test]
    fn srt_output_has_expected_timestamps() {
        let output = render_srt(&manifest().captions);
        assert!(output.contains("00:00:00,000 --> 00:00:01,000"));
    }

    #[test]
    fn captions_are_short_and_never_overlap() {
        let transcript = vec![TranscriptSegment {
            session_id: "session".to_owned(),
            start_ms: 1_000,
            end_ms: 9_000,
            text: "The same golem wandered the end until it was reachable and every shot still had to line up"
                .to_owned(),
            confidence: Some(0.9),
            no_speech_probability: None,
            is_final: true,
        }];
        let captions = build_caption_cues(&transcript, 1_000, 9_000);

        assert!(captions.len() > 1);
        assert!(captions.iter().all(|cue| {
            cue.text.split_whitespace().count() <= MAX_CAPTION_WORDS
                && cue.text.chars().count() <= MAX_CAPTION_CHARACTERS
        }));
        assert!(
            captions
                .windows(2)
                .all(|pair| pair[0].end_ms <= pair[1].start_ms)
        );
    }

    #[test]
    fn overlapping_transcript_windows_do_not_repeat_words_or_cues() {
        let segment = |start_ms, end_ms, text: &str| TranscriptSegment {
            session_id: "session".to_owned(),
            start_ms,
            end_ms,
            text: text.to_owned(),
            confidence: Some(0.9),
            no_speech_probability: None,
            is_final: true,
        };
        let transcript = vec![
            segment(0, 5_000, "we still had to line up"),
            segment(4_000, 9_000, "still had to line up for the shot"),
        ];
        let captions = build_caption_cues(&transcript, 0, 10_000);
        let displayed_text = captions
            .iter()
            .map(|cue| cue.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        assert_eq!(displayed_text, "we still had to line up for the shot");
        assert!(
            captions
                .windows(2)
                .all(|pair| pair[0].end_ms <= pair[1].start_ms)
        );
    }

    #[test]
    fn ass_uses_explicit_canvas_and_separate_safe_zones() {
        let output = render_ass(&manifest().captions, Some("Wait for it"), 10_000);

        assert!(output.contains("PlayResX: 1080\nPlayResY: 1920"));
        assert!(output.contains("Style: Caption,Arial,64"));
        assert!(output.contains(",2,110,110,430,1"));
        assert!(output.contains("Style: Hook,Arial,54"));
        assert!(output.contains(",8,120,120,150,1"));
        assert!(output.contains("Dialogue: 1,0:00:00.00,0:00:03.00,Hook"));
    }
}
