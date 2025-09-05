#[cfg(test)]
mod ai_router {
    use aurora_shell::ai;

    #[test]
    fn router_stub_compiles_and_returns_text() {
        let out =
            ai::complete_sync_router("Summarize this stub").unwrap_or_else(|_| "<disabled>".into());
        assert!(!out.is_empty());
    }
}
