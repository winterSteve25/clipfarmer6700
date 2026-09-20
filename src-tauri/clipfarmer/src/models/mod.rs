//! Hosted model providers behind the provider-neutral editorial traits.

pub mod gemini;
pub mod openai;

use crate::{
    domain::{AudioAnnotation, Candidate, VisualSample},
    manifest::validate_safe_path,
};
use anyhow::{Context, Result, ensure};
use std::{collections::HashSet, fs, path::Path};

pub(crate) async fn extract_candidate_audio(
    ffmpeg: &Path,
    work_dir: &Path,
    input_path: &str,
    candidate: &Candidate,
    sample_rate: u32,
) -> Result<Vec<u8>> {
    validate_safe_path(input_path)?;
    fs::create_dir_all(work_dir)?;
    let wav = work_dir.join(format!("candidate-audio-{}.wav", uuid::Uuid::new_v4()));
    let extracted = tokio::process::Command::new(ffmpeg)
        .args(["-y", "-v", "error", "-ss"])
        .arg(seconds(candidate.start_ms))
        .args(["-t"])
        .arg(seconds(candidate.duration_ms()))
        .args(["-i", input_path, "-vn", "-ac", "1", "-ar"])
        .arg(sample_rate.to_string())
        .arg(&wav)
        .kill_on_drop(true)
        .output()
        .await
        .context("extract candidate audio")?;
    if !extracted.status.success() {
        let _ = fs::remove_file(&wav);
        anyhow::bail!(
            "candidate audio extraction failed: {}",
            String::from_utf8_lossy(&extracted.stderr)
        );
    }
    let audio = fs::read(&wav).context("read extracted candidate audio");
    let _ = fs::remove_file(&wav);
    audio
}

pub(crate) fn validate_audio_annotation(annotation: &AudioAnnotation) -> Result<()> {
    ensure!(
        annotation.confidence.is_finite() && (0.0..=1.0).contains(&annotation.confidence),
        "invalid audio confidence"
    );
    Ok(())
}

pub(crate) fn select_visuals(samples: &[VisualSample], limit: usize) -> Vec<&VisualSample> {
    let mut timestamps = HashSet::new();
    let unique = samples
        .iter()
        .filter(|sample| timestamps.insert(sample.at_ms))
        .collect::<Vec<_>>();
    if unique.len() <= limit {
        return unique;
    }
    if limit <= 1 {
        return unique.last().copied().into_iter().collect();
    }
    (0..limit)
        .map(|index| {
            let position = index * (unique.len() - 1) / (limit - 1);
            unique[position]
        })
        .collect()
}

pub(crate) fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(TABLE[(value >> 18) as usize] as char);
        output.push(TABLE[((value >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(value & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn seconds(ms: i64) -> String {
    format!("{:.3}", ms as f64 / 1_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_selection_keeps_both_story_ends() {
        let samples = (0..100)
            .map(|index| VisualSample {
                at_ms: index,
                path: index.to_string(),
                region: "full_frame".to_owned(),
                reason: "baseline".to_owned(),
            })
            .collect::<Vec<_>>();
        let selected = select_visuals(&samples, 24);
        assert_eq!(selected.first().unwrap().at_ms, 0);
        assert_eq!(selected.last().unwrap().at_ms, 99);
        assert_eq!(selected.len(), 24);
    }

    #[test]
    fn visual_selection_deduplicates_overlapping_window_samples() {
        let samples = [0, 1, 1, 2, 2, 3]
            .into_iter()
            .enumerate()
            .map(|(index, at_ms)| VisualSample {
                at_ms,
                path: index.to_string(),
                region: "full_frame".to_owned(),
                reason: "baseline".to_owned(),
            })
            .collect::<Vec<_>>();
        let selected = select_visuals(&samples, 24);
        assert_eq!(
            selected
                .iter()
                .map(|sample| sample.at_ms)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
    }
}
