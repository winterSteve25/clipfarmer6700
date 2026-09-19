use crate::domain::{Candidate, CaptionCue, EditManifest, EditorialDecision, TranscriptSegment};
use anyhow::{Result, bail, ensure};
use std::path::{Component, Path};

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
    let captions = transcript
        .iter()
        .filter_map(|segment| {
            let cue_start = segment.start_ms.max(start_ms) - start_ms;
            let cue_end = segment.end_ms.min(end_ms) - start_ms;
            (cue_end > cue_start).then(|| CaptionCue {
                start_ms: cue_start,
                end_ms: cue_end,
                text: segment.text.clone(),
            })
        })
        .collect();
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
}
