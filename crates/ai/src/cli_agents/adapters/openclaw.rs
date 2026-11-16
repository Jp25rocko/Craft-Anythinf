//! OpenClaw, driven over ACP through its own bridge mode.
//!
//! `openclaw acp` exposes an OpenClaw Gateway session as an ACP server over stdio, so an
//! ACP-aware client can forward prompts to it. This adapter is therefore ACP-only, like
//! Copilot's: a non-ACP run is refused instead of invoking the bridge with a bare prompt.
//! Install OpenClaw (`npm install -g openclaw`), sign in / configure its providers, and
//! enable ACP in it; OpenClaw owns its sign-in and provider connections, Craft Anything
//! never sees them. An ACP session is non-interactive: OpenClaw can't ask the person in
//! the middle of a run, so approvals go through Craft Anything's own channel.
//!
//! Not yet run live against `openclaw acp`: the ACP shape follows OpenClaw's documented
//! bridge (`openclaw acp`, stdio), and the adapter, detection and grants-table rows are
//! covered by unit tests. Verify with a live run before trusting a build to it.

use super::*;

pub struct OpenClaw;

impl AgentAdapter for OpenClaw {
    fn ids(&self) -> &'static [&'static str] {
        &["openclaw"]
    }

    fn args(&self, _cx: &ArgsCx) -> Vec<String> {
        // The native runner refuses `requires_acp` before it asks for arguments.
        Vec::new()
    }

    fn requires_acp(&self) -> bool {
        true
    }

    fn plain_fallback(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openclaw_is_acp_only_and_registered() {
        assert!(OpenClaw.requires_acp(), "openclaw acp is the ACP bridge, so every run of it is one");
        assert!(!OpenClaw.plain_fallback());
        assert!(AgentAdapters::built_in().for_agent("openclaw").ids().contains(&"openclaw"), "registered, not the plain adapter");
        assert_eq!(acp::acp_args("openclaw"), Some(["acp"].as_slice()));
    }
}
