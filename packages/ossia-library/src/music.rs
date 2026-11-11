use crate::*;

impl Library {
    /// Compose evidence without assigning musical meaning to conversion success.
    pub fn music_report(
        &self,
        snapshot: &SourceSnapshot,
        render: &RenderManifest,
    ) -> Result<MusicReport> {
        let performance = match &render.midi {
            Some(asset) => ossia_score::analyze_performance(&ossia_score::parse_midi(
                &self.read_artifact(asset)?,
            )?),
            None => ossia_score::unavailable_performance(
                render
                    .format_errors
                    .iter()
                    .find(|e| e.format == RenderFormat::Midi)
                    .map(|e| e.error.message.as_str())
                    .unwrap_or("MIDI was not requested or produced"),
            ),
        };
        Ok(MusicReport {
            source: ossia_score::analyze_source(snapshot.source()),
            performance,
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/music.rs"]
mod tests;
