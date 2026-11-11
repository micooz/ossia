use crate::agent::AgentAdapter;
use crate::{codex, dsh};

static CODEX: codex::Codex = codex::Codex;
static DSH: dsh::Dsh = dsh::Dsh;
static ADAPTERS: [&'static dyn AgentAdapter; 2] = [&CODEX, &DSH];

pub(crate) fn adapters() -> &'static [&'static dyn AgentAdapter] {
    &ADAPTERS
}

pub(crate) fn find(id: &str) -> Option<&'static dyn AgentAdapter> {
    adapters()
        .iter()
        .copied()
        .find(|adapter| adapter.id() == id)
}
