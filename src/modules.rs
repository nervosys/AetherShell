//! Module system for AetherShell builtins.
//!
//! This module provides namespaced access to builtin functions:
//! - `sys.info()`, `sys.hostname()`, `sys.uptime()`
//! - `net.interfaces()`, `net.ping()`, `net.dns_lookup()`
//! - `gui.screenshot()`, `gui.click()`, `gui.type_text()`
//! - `crypto.hash()`, `crypto.encrypt()`, `crypto.sign()`
//! - etc.

use crate::value::{BuiltinRef, Value};
use std::collections::BTreeMap;

/// Create a builtin reference value
pub fn builtin_ref(name: &str) -> Value {
    Value::Builtin(BuiltinRef {
        name: name.to_string(),
    })
}

/// Create a module record with the given functions
fn module(functions: &[(&str, &str)]) -> Value {
    let mut map = BTreeMap::new();
    for (fn_name, builtin_name) in functions {
        map.insert(fn_name.to_string(), builtin_ref(builtin_name));
    }
    Value::Record(map)
}

/// System module: sys.info(), sys.hostname(), sys.uptime(), etc.
pub fn sys_module() -> Value {
    module(&[
        ("info", "sys_info"),
        ("hostname", "sys_hostname"),
        ("os", "sys_os"),
        ("arch", "sys_arch"),
        ("kernel", "sys_kernel"),
        ("uptime", "sys_uptime"),
        ("boot_time", "sys_boot_time"),
        ("cpu_info", "sys_cpu_info"),
        ("cpu_count", "sys_cpu_count"),
        ("cpu_freq", "sys_cpu_freq"),
        ("mem_info", "sys_mem_info"),
        ("swap_info", "sys_swap_info"),
        ("disk_info", "sys_disk_info"),
        ("load_avg", "sys_load_avg"),
        ("users", "sys_users"),
        ("groups", "sys_groups"),
        ("user_info", "sys_user_info"),
        ("env_all", "sys_env_all"),
        ("locale", "sys_locale"),
        ("timezone", "sys_timezone"),
    ])
}

/// Process module: proc.list(), proc.kill(), proc.info(), etc.
pub fn proc_module() -> Value {
    module(&[
        ("list", "proc_list"),
        ("kill", "proc_kill"),
        ("info", "proc_info"),
        ("spawn", "proc_spawn"),
        ("wait", "proc_wait"),
        ("exists", "proc_exists"),
        ("children", "proc_children"),
        ("parent", "proc_parent"),
        ("priority", "proc_priority"),
        ("set_priority", "proc_set_priority"),
        ("cpu_usage", "proc_cpu_usage"),
        ("mem_usage", "proc_mem_usage"),
        ("threads", "proc_threads"),
        ("env", "proc_env"),
        ("cwd", "proc_cwd"),
        ("exe", "proc_exe"),
        ("cmdline", "proc_cmdline"),
        ("start_time", "proc_start_time"),
        ("suspend", "proc_suspend"),
        ("resume", "proc_resume"),
    ])
}

/// Filesystem module: fs.stat(), fs.chmod(), fs.glob(), etc.
pub fn fs_module() -> Value {
    module(&[
        ("stat", "fs_stat"),
        ("lstat", "fs_lstat"),
        ("chmod", "fs_chmod"),
        ("chown", "fs_chown"),
        ("link", "fs_link"),
        ("symlink", "fs_symlink"),
        ("readlink", "fs_readlink"),
        ("realpath", "fs_realpath"),
        ("tempfile", "fs_tempfile"),
        ("tempdir", "fs_tempdir"),
        ("watch", "fs_watch"),
        ("unwatch", "fs_unwatch"),
        ("glob", "fs_glob"),
        ("walk", "fs_walk"),
        ("tree", "fs_tree"),
        ("du", "fs_du"),
        ("df", "fs_df"),
        ("mount", "fs_mount"),
        ("unmount", "fs_unmount"),
        ("mounts", "fs_mounts"),
        // Also include basic file ops
        ("read", "cat"),
        ("read_text", "read_text"),
        ("write", "save_json"),
        ("ls", "ls"),
        ("find", "find"),
        // Robust file editing (for AI agents)
        ("write_text", "file_write"),
        ("append", "file_append"),
        ("patch", "file_patch"),
        ("replace", "file_replace"),
        ("insert", "file_insert"),
        ("delete_lines", "file_delete_lines"),
        ("edit", "file_edit"),
        ("diff", "file_diff"),
    ])
}

/// File module (dedicated): file.write(), file.backup(), file.patch(), etc.
pub fn file_module() -> Value {
    module(&[
        ("read", "cat"),
        ("write", "file_write"),
        ("append", "file_append"),
        ("exists", "file_exists"),
        ("patch", "file_patch"),
        ("replace", "file_replace"),
        ("insert", "file_insert"),
        ("delete_lines", "file_delete_lines"),
        ("edit", "file_edit"),
        ("diff", "file_diff"),
        ("backup", "file_backup"),
        ("copy", "file_copy"),
        ("move", "file_move"),
        ("mkdir", "file_mkdir"),
    ])
}

/// Network module: net.interfaces(), net.ping(), net.dns_lookup(), etc.
pub fn net_module() -> Value {
    module(&[
        ("interfaces", "net_interfaces"),
        ("ip", "net_ip"),
        ("dns_lookup", "net_dns_lookup"),
        ("dns_reverse", "net_dns_reverse"),
        ("ping", "net_ping"),
        ("traceroute", "net_traceroute"),
        ("connections", "net_connections"),
        ("listen", "net_listen"),
        ("connect", "net_connect"),
        ("send", "net_send"),
        ("recv", "net_recv"),
        ("close", "net_close"),
        ("ports", "net_ports"),
        ("arp", "net_arp"),
        ("route", "net_route"),
        ("stats", "net_stats"),
        ("bandwidth", "net_bandwidth"),
        ("latency", "net_latency"),
        ("scan", "net_scan"),
        ("whois", "net_whois"),
    ])
}

/// HTTP module: http.get(), http.post(), http.json(), etc.
pub fn http_module() -> Value {
    module(&[
        ("get", "http_get"),
        ("post", "web_post"),
        ("json_get", "web_json_get"),
        ("json_post", "web_json_post"),
        ("download", "web_download"),
        ("fetch", "web_fetch"),
        ("headers", "web_headers"),
    ])
}

/// GUI Automation module: gui.screenshot(), gui.click(), gui.type_text(), etc.
pub fn gui_module() -> Value {
    module(&[
        ("screenshot", "gui_screenshot"),
        ("screen_size", "gui_screen_size"),
        ("mouse_position", "gui_mouse_position"),
        ("mouse_move", "gui_mouse_move"),
        ("click", "gui_click"),
        ("double_click", "gui_double_click"),
        ("right_click", "gui_right_click"),
        ("drag", "gui_drag"),
        ("scroll", "gui_scroll"),
        ("type_text", "gui_type_text"),
        ("key_press", "gui_key_press"),
        ("key_down", "gui_key_down"),
        ("key_up", "gui_key_up"),
        ("hotkey", "gui_hotkey"),
        ("window_list", "gui_window_list"),
        ("window_focus", "gui_window_focus"),
        ("window_close", "gui_window_close"),
        ("window_minimize", "gui_window_minimize"),
        ("window_maximize", "gui_window_maximize"),
        ("window_resize", "gui_window_resize"),
        ("window_move", "gui_window_move"),
        ("find_image", "gui_find_image"),
        ("wait_image", "gui_wait_image"),
        ("ocr", "gui_ocr"),
        ("ocr_region", "gui_ocr_region"),
        ("color_at", "gui_color_at"),
        ("pixel_search", "gui_pixel_search"),
        ("clipboard_get", "clipboard_get"),
        ("clipboard_set", "clipboard_set"),
    ])
}

/// Web Automation module: web.fetch(), web.screenshot(), web.navigate(), etc.
pub fn web_module() -> Value {
    module(&[
        ("fetch", "web_fetch"),
        ("get", "web_get"),
        ("post", "web_post"),
        ("json_get", "web_json_get"),
        ("json_post", "web_json_post"),
        ("download", "web_download"),
        ("screenshot", "web_screenshot"),
        ("navigate", "web_navigate"),
        ("click", "web_click"),
        ("type_text", "web_type"),
        ("select", "web_select"),
        ("extract", "web_extract"),
        ("extract_all", "web_extract_all"),
        ("wait", "web_wait"),
        ("eval_js", "web_eval"),
        ("cookies", "web_cookies"),
        ("headers", "web_headers"),
        ("form_fill", "web_form_fill"),
        ("form_submit", "web_form_submit"),
        ("scroll", "web_scroll"),
        ("back", "web_back"),
        ("forward", "web_forward"),
        ("refresh", "web_refresh"),
        ("pdf", "web_pdf"),
        ("links", "web_links"),
        ("table_extract", "web_table_extract"),
    ])
}

/// Crypto module: crypto.hash(), crypto.encrypt(), crypto.sign(), etc.
pub fn crypto_module() -> Value {
    module(&[
        ("hash", "crypto_hash"),
        ("hmac", "crypto_hmac"),
        ("random_bytes", "crypto_random_bytes"),
        ("uuid", "crypto_uuid"),
        ("encrypt", "crypto_encrypt"),
        ("decrypt", "crypto_decrypt"),
        ("sign", "crypto_sign"),
        ("verify", "crypto_verify"),
        ("key_generate", "crypto_key_generate"),
        ("key_derive", "crypto_key_derive"),
        ("base64_encode", "crypto_base64_encode"),
        ("base64_decode", "crypto_base64_decode"),
        ("hex_encode", "crypto_hex_encode"),
        ("hex_decode", "crypto_hex_decode"),
        ("password_hash", "crypto_password_hash"),
        ("password_verify", "crypto_password_verify"),
        ("cert_info", "crypto_cert_info"),
        ("cert_verify", "crypto_cert_verify"),
        ("jwt_encode", "crypto_jwt_encode"),
        ("jwt_decode", "crypto_jwt_decode"),
    ])
}

/// Database module: db.sqlite_open(), db.sqlite_query(), etc.
pub fn db_module() -> Value {
    module(&[
        ("sqlite_open", "db_sqlite_open"),
        ("sqlite_close", "db_sqlite_close"),
        ("sqlite_query", "db_sqlite_query"),
        ("sqlite_exec", "db_sqlite_exec"),
        ("sqlite_insert", "db_sqlite_insert"),
        ("sqlite_update", "db_sqlite_update"),
        ("sqlite_delete", "db_sqlite_delete"),
        ("sqlite_tables", "db_sqlite_tables"),
        ("sqlite_schema", "db_sqlite_schema"),
        ("sqlite_transaction", "db_sqlite_transaction"),
        ("sqlite_commit", "db_sqlite_commit"),
        ("sqlite_rollback", "db_sqlite_rollback"),
        ("kv_open", "db_kv_open"),
        ("kv_get", "db_kv_get"),
        ("kv_set", "db_kv_set"),
        ("kv_delete", "db_kv_delete"),
        ("kv_list", "db_kv_list"),
        ("kv_close", "db_kv_close"),
        ("redis_connect", "db_redis_connect"),
        ("redis_cmd", "db_redis_cmd"),
    ])
}

/// Service module: svc.list(), svc.start(), svc.stop(), etc.
pub fn svc_module() -> Value {
    module(&[
        ("list", "svc_list"),
        ("status", "svc_status"),
        ("start", "svc_start"),
        ("stop", "svc_stop"),
        ("restart", "svc_restart"),
        ("enable", "svc_enable"),
        ("disable", "svc_disable"),
        ("logs", "svc_logs"),
        ("info", "svc_info"),
        ("create", "svc_create"),
        ("delete", "svc_delete"),
    ])
}

/// Cron/scheduler module: cron.list(), cron.add(), etc.
pub fn cron_module() -> Value {
    module(&[
        ("list", "cron_list"),
        ("add", "cron_add"),
        ("remove", "cron_remove"),
        ("enable", "cron_enable"),
        ("disable", "cron_disable"),
        ("at_schedule", "at_schedule"),
        ("at_list", "at_list"),
        ("at_remove", "at_remove"),
        ("startup_list", "startup_list"),
    ])
}

/// Archive module: archive.zip_create(), archive.tar_extract(), etc.
pub fn archive_module() -> Value {
    module(&[
        ("zip_create", "zip_create"),
        ("zip_extract", "zip_extract"),
        ("zip_list", "zip_list"),
        ("zip_add", "zip_add"),
        ("tar_create", "tar_create"),
        ("tar_extract", "tar_extract"),
        ("tar_list", "tar_list"),
        ("gzip", "gzip_compress"),
        ("gunzip", "gzip_decompress"),
        ("bzip2", "bzip2_compress"),
        ("bunzip2", "bzip2_decompress"),
        ("xz", "xz_compress"),
        ("unxz", "xz_decompress"),
        ("zstd", "zstd_compress"),
        ("unzstd", "zstd_decompress"),
        ("info", "archive_info"),
        ("test", "archive_test"),
    ])
}

/// User management module: user.list(), user.add(), etc.
pub fn user_module() -> Value {
    module(&[
        ("list", "user_list"),
        ("add", "user_add"),
        ("del", "user_del"),
        ("mod", "user_mod"),
        ("passwd", "user_passwd"),
        ("lock", "user_lock"),
        ("unlock", "user_unlock"),
        ("group_list", "group_list"),
        ("group_add", "group_add"),
        ("group_del", "group_del"),
        ("group_mod", "group_mod"),
        ("group_members", "group_members"),
    ])
}

/// Permission module: perm.get(), perm.set(), etc.
pub fn perm_module() -> Value {
    module(&[
        ("get", "perm_get"),
        ("set", "perm_set"),
        ("acl_get", "perm_acl_get"),
        ("acl_set", "perm_acl_set"),
        ("owner_get", "perm_owner_get"),
        ("owner_set", "perm_owner_set"),
    ])
}

/// Package manager module: pkg.list(), pkg.install(), etc.
pub fn pkg_module() -> Value {
    module(&[
        ("list", "pkg_list"),
        ("install", "pkg_install"),
        ("uninstall", "pkg_uninstall"),
        ("update", "pkg_update"),
        ("upgrade", "pkg_upgrade"),
        ("search", "pkg_search"),
        ("info", "pkg_info"),
        ("outdated", "pkg_outdated"),
        ("clean", "pkg_clean"),
        ("repos", "pkg_repos"),
        ("repo_add", "pkg_repo_add"),
        ("repo_remove", "pkg_repo_remove"),
        ("files", "pkg_files"),
        ("owner", "pkg_owner"),
        ("verify", "pkg_verify"),
        ("changelog", "pkg_changelog"),
        ("deps", "pkg_deps"),
        ("rdeps", "pkg_rdeps"),
        ("lock", "pkg_lock"),
        ("unlock", "pkg_unlock"),
    ])
}

/// Hardware module: hw.cpu(), hw.memory(), hw.disk(), etc.
pub fn hw_module() -> Value {
    module(&[
        ("cpu", "hw_cpu"),
        ("memory", "hw_memory"),
        ("disk", "hw_disk"),
        ("gpu", "hw_gpu"),
        ("battery", "hw_battery"),
        ("sensors", "hw_sensors"),
        ("usb", "hw_usb"),
        ("pci", "hw_pci"),
        ("network", "hw_network"),
        ("audio", "hw_audio"),
    ])
}

/// Clipboard module: clip.get(), clip.set(), clip.clear(), etc.
pub fn clip_module() -> Value {
    module(&[
        ("get", "clipboard_get"),
        ("set", "clipboard_set"),
        ("clear", "clipboard_clear"),
        ("has_text", "clipboard_has_text"),
        ("has_image", "clipboard_has_image"),
        ("get_image", "clipboard_get_image"),
        ("set_image", "clipboard_set_image"),
        ("history", "clipboard_history"),
        ("watch", "clipboard_watch"),
    ])
}

/// Input module: input.read_line(), input.confirm(), etc.
pub fn input_module() -> Value {
    module(&[
        ("read_line", "input_read_line"),
        ("read_password", "input_read_password"),
        ("confirm", "input_confirm"),
        ("select", "input_select"),
        ("multi_select", "input_multi_select"),
        ("text", "input_text"),
        ("number", "input_number"),
        ("path", "input_path"),
        ("editor", "input_editor"),
    ])
}

/// AI module: ai.chat(), ai.embed(), ai.agent(), etc.
pub fn ai_module() -> Value {
    module(&[
        ("chat", "ai"),
        ("embed", "embed"),
        ("agent", "agent"),
        ("swarm", "swarm"),
        ("backends", "ai_backends"),
        ("detect", "ai_detect"),
        // RAG functions
        ("rag_index", "rag_index"),
        ("rag_search", "rag_search"),
        ("rag_query", "rag_query"),
        ("rag_clear", "rag_clear"),
        // Knowledge graph
        ("kg_add", "kg_add"),
        ("kg_relate", "kg_relate"),
        ("kg_query", "kg_query"),
        ("kg_entities", "kg_entities"),
        ("kg_relations", "kg_relations"),
    ])
}

/// Math module: math.abs(), math.sqrt(), math.sin(), etc.

/// Agent module: agent.with_mcp(), agent.run(), etc.
pub fn agent_module() -> Value {
    module(&[("run", "agent"), ("with_mcp", "agent_with_mcp")])
}
pub fn math_module() -> Value {
    module(&[
        ("abs", "abs"),
        ("min", "min"),
        ("max", "max"),
        ("floor", "floor"),
        ("ceil", "ceil"),
        ("round", "round"),
        ("sqrt", "sqrt"),
        ("pow", "pow"),
        ("sum", "sum"),
        ("avg", "avg"),
        ("product", "product"),
    ])
}

/// String module: str.split(), str.join(), str.trim(), etc.
pub fn str_module() -> Value {
    module(&[
        ("split", "split"),
        ("join", "join"),
        ("trim", "trim"),
        ("upper", "upper"),
        ("lower", "lower"),
        ("replace", "replace"),
        ("contains", "contains"),
        ("starts_with", "starts_with"),
        ("ends_with", "ends_with"),
        ("len", "len"),
    ])
}

/// Array module: arr.map(), arr.filter(), arr.reduce(), etc.
pub fn arr_module() -> Value {
    module(&[
        ("map", "map"),
        ("filter", "where"),
        ("reduce", "reduce"),
        ("take", "take"),
        ("first", "first"),
        ("last", "last"),
        ("any", "any"),
        ("all", "all"),
        ("flatten", "flatten"),
        ("reverse", "reverse"),
        ("slice", "slice"),
        ("range", "range"),
        ("zip", "zip"),
        ("push", "push"),
        ("concat", "concat"),
        ("sort", "sort"),
        ("sort_by", "sort_by"),
        ("unique", "unique"),
        ("len", "len"),
        ("keys", "keys"),
        ("values", "values"),
    ])
}

/// JSON module: json.parse(), json.stringify(), etc.
pub fn json_module() -> Value {
    module(&[
        ("parse", "json_parse"),
        ("stringify", "json_stringify"),
        ("save", "save_json"),
    ])
}

/// MCP (Model Context Protocol) module
pub fn mcp_module() -> Value {
    module(&[
        ("servers", "mcp_servers"),
        ("detect", "mcp_detect"),
        ("cache_clear", "mcp_cache_clear"),
        ("cache_status", "mcp_cache_status"),
        ("server", "mcp_server"),
        ("tools", "mcp_tools"),
        ("call", "mcp_call"),
        ("resources", "mcp_resources"),
        ("prompts", "mcp_prompts"),
        ("connect", "mcp_connect"),
        ("server_start", "mcp_server_start"),
    ])
}

/// Shell utilities module
pub fn shell_module() -> Value {
    module(&[
        ("sh", "sh"),
        ("cd", "cd"),
        ("pwd", "pwd"),
        ("ls", "ls"),
        ("cat", "cat"),
        ("echo", "echo"),
        ("clear", "clear"),
        ("exit", "exit"),
        ("env", "env"),
        ("set_env", "set_env"),
        ("sleep", "sleep"),
        ("time", "time"),
    ])
}

/// A2UI (Agent-to-User Interface) module
pub fn a2ui_module() -> Value {
    module(&[
        ("notify", "a2ui_notify"),
        ("prompt", "a2ui_prompt"),
        ("confirm", "a2ui_confirm"),
        ("select", "a2ui_select"),
        ("progress", "a2ui_progress"),
        ("progress_complete", "a2ui_progress_complete"),
        ("render", "a2ui_render"),
        ("render_table", "a2ui_render_table"),
        ("render_code", "a2ui_render_code"),
        ("toast", "a2ui_toast"),
        ("status", "a2ui_status"),
        ("clear", "a2ui_clear"),
        ("agent_started", "a2ui_agent_started"),
        ("agent_completed", "a2ui_agent_completed"),
        ("agent_thinking", "a2ui_agent_thinking"),
    ])
}

/// Distributed computing module
pub fn cluster_module() -> Value {
    module(&[
        ("create", "cluster_create"),
        ("add_node", "cluster_add_node"),
        ("remove_node", "cluster_remove_node"),
        ("status", "cluster_status"),
        ("nodes", "cluster_nodes"),
        ("job_submit", "job_submit"),
        ("job_status", "job_status"),
        ("job_cancel", "job_cancel"),
        ("job_results", "job_results"),
        ("job_list", "job_list"),
        ("remote_exec", "remote_exec"),
        ("aggregate_results", "aggregate_results"),
    ])
}

/// Cloud platform module — hosted AetherShell instances
pub fn cloud_module() -> Value {
    module(&[
        ("deploy", "cloud_deploy"),
        ("instances", "cloud_instances"),
        ("instance_create", "cloud_instance_create"),
        ("instance_destroy", "cloud_instance_destroy"),
        ("instance_status", "cloud_instance_status"),
        ("instance_connect", "cloud_instance_connect"),
        ("regions", "cloud_regions"),
        ("config", "cloud_config"),
    ])
}

/// Remote REPL module — WebSocket-based remote shell sessions
pub fn repl_module() -> Value {
    module(&[
        ("serve", "repl_serve"),
        ("connect", "repl_connect"),
        ("sessions", "repl_sessions"),
        ("disconnect", "repl_disconnect"),
        ("broadcast", "repl_broadcast"),
    ])
}

/// Team workspace module — shared workspaces with agents
pub fn workspace_module() -> Value {
    module(&[
        ("create", "workspace_create"),
        ("list", "workspace_list"),
        ("join", "workspace_join"),
        ("leave", "workspace_leave"),
        ("members", "workspace_members"),
        ("share_agent", "workspace_share_agent"),
        ("agents", "workspace_agents"),
        ("sync", "workspace_sync"),
    ])
}

/// Marketplace module — community plugins and agents
pub fn marketplace_module() -> Value {
    module(&[
        ("publish", "marketplace_publish"),
        ("search", "marketplace_search"),
        ("install", "marketplace_install"),
        ("uninstall", "marketplace_uninstall"),
        ("list", "marketplace_list"),
        ("info", "marketplace_info"),
        ("rate", "marketplace_rate"),
        ("update", "marketplace_update"),
    ])
}

/// Telemetry module — opt-in usage analytics
pub fn telemetry_module() -> Value {
    module(&[
        ("enable", "telemetry_enable"),
        ("disable", "telemetry_disable"),
        ("status", "telemetry_status"),
        ("report", "telemetry_report"),
        ("events", "telemetry_events"),
        ("reset", "telemetry_reset"),
    ])
}

/// Neural network module
pub fn nn_module() -> Value {
    module(&[
        ("create", "nn_create"),
        ("forward", "nn_forward"),
        ("mutate", "nn_mutate"),
        ("crossover", "nn_crossover"),
        ("params", "nn_params"),
        ("set_params", "nn_set_params"),
        ("layers", "nn_layers"),
        ("info", "nn_info"),
        ("activation", "activation"),
    ])
}

/// Evolution/genetic algorithm module
pub fn evo_module() -> Value {
    module(&[
        ("population", "population"),
        ("evolve", "evolve"),
        ("evolve_step", "evolve_step"),
        ("fitness", "fitness"),
        ("best", "best_individual"),
        ("stats", "evolution_stats"),
        ("selection", "selection_strategy"),
        ("crossover", "crossover_strategy"),
        ("config", "evolution_config"),
        ("coevolve", "coevolve"),
    ])
}

/// Reinforcement learning module
pub fn rl_module() -> Value {
    module(&[
        ("agent", "rl_agent"),
        ("action", "rl_action"),
        ("update", "rl_update"),
        ("sarsa_agent", "rl_sarsa_agent"),
        ("sarsa_update", "rl_sarsa_update"),
        ("pg_agent", "rl_pg_agent"),
        ("pg_step", "rl_pg_step"),
        ("pg_episode_end", "rl_pg_episode_end"),
        ("ac_agent", "rl_ac_agent"),
        ("ac_update", "rl_ac_update"),
        ("dqn_agent", "rl_dqn_agent"),
        ("dqn_step", "rl_dqn_step"),
        ("replay_buffer", "rl_replay_buffer"),
        ("gridworld", "rl_gridworld"),
        ("env_step", "rl_env_step"),
    ])
}

/// A2A (Agent-to-Agent) protocol module
pub fn a2a_module() -> Value {
    module(&[
        ("send", "a2a_send"),
        ("receive", "a2a_receive"),
        ("broadcast", "a2a_broadcast"),
        ("discover", "a2a_discover"),
        ("register", "a2a_register"),
        ("unregister", "a2a_unregister"),
        ("status", "a2a_status"),
        ("agents", "a2a_agents"),
    ])
}

/// NANDA (consensus) protocol module
pub fn nanda_module() -> Value {
    module(&[
        ("propose", "nanda_propose"),
        ("vote", "nanda_vote"),
        ("commit", "nanda_commit"),
        ("abort", "nanda_abort"),
        ("status", "nanda_status"),
        ("consensus", "nanda_consensus"),
        ("quorum", "nanda_quorum"),
    ])
}

/// RBAC (Role-Based Access Control) module
pub fn rbac_module() -> Value {
    module(&[
        ("create", "rbac_create_role"),
        ("delete", "rbac_delete_role"),
        ("grant", "rbac_grant"),
        ("revoke", "rbac_revoke"),
        ("check", "rbac_check"),
        ("roles", "rbac_roles"),
        ("permissions", "rbac_permissions"),
        ("user_roles", "rbac_user_roles"),
    ])
}

/// Audit logging module
pub fn audit_module() -> Value {
    module(&[
        ("log", "audit_log"),
        ("query", "audit_query"),
        ("export", "audit_export"),
        ("retention", "audit_retention"),
        ("stream", "audit_stream"),
    ])
}

/// SSO (Single Sign-On) module
pub fn sso_module() -> Value {
    module(&[
        ("init", "sso_init"),
        ("auth", "sso_auth"),
        ("refresh", "sso_refresh"),
        ("logout", "sso_logout"),
        ("validate", "sso_validate"),
        ("user_info", "sso_user_info"),
        ("providers", "sso_providers"),
    ])
}

// ============== AI CODING ASSISTANT MODULES ==============

/// Git operations module for AI coding assistants
pub fn git_module() -> Value {
    module(&[
        ("status", "git_status"),
        ("diff", "git_diff"),
        ("diff_staged", "git_diff_staged"),
        ("log", "git_log"),
        ("blame", "git_blame"),
        ("branch", "git_branch"),
        ("branches", "git_branches"),
        ("checkout", "git_checkout"),
        ("commit", "git_commit"),
        ("add", "git_add"),
        ("reset", "git_reset"),
        ("stash", "git_stash"),
        ("stash_pop", "git_stash_pop"),
        ("stash_list", "git_stash_list"),
        ("remote", "git_remote"),
        ("fetch", "git_fetch"),
        ("pull", "git_pull"),
        ("push", "git_push"),
        ("merge", "git_merge"),
        ("rebase", "git_rebase"),
        ("cherry_pick", "git_cherry_pick"),
        ("tags", "git_tags"),
        ("show", "git_show"),
        ("rev_parse", "git_rev_parse"),
        ("root", "git_root"),
        ("ignore", "git_ignore"),
        ("clean", "git_clean"),
    ])
}

/// Code analysis module for AI coding assistants
pub fn code_module() -> Value {
    module(&[
        ("parse", "code_parse"),
        ("symbols", "code_symbols"),
        ("outline", "code_outline"),
        ("references", "code_references"),
        ("definition", "code_definition"),
        ("imports", "code_imports"),
        ("exports", "code_exports"),
        ("dependencies", "code_dependencies"),
        ("callers", "code_callers"),
        ("callees", "code_callees"),
        ("hover", "code_hover"),
        ("signature", "code_signature"),
        ("completions", "code_completions"),
        ("format", "code_format"),
        ("language", "code_language"),
        ("comments", "code_comments"),
        ("todos", "code_todos"),
    ])
}

/// Project structure module for AI coding assistants
pub fn project_module() -> Value {
    module(&[
        ("ptype", "project_type"),
        ("root", "project_root"),
        ("name", "project_name"),
        ("version", "project_version"),
        ("dependencies", "project_dependencies"),
        ("dev_dependencies", "project_dev_dependencies"),
        ("scripts", "project_scripts"),
        ("structure", "project_structure"),
        ("entry_points", "project_entry_points"),
        ("test_files", "project_test_files"),
        ("config_files", "project_config_files"),
        ("readme", "project_readme"),
        ("license", "project_license"),
        ("gitignore", "project_gitignore"),
        ("languages", "project_languages"),
        ("loc", "project_loc"),
        ("size", "project_size"),
    ])
}

/// Code search module for AI coding assistants  
pub fn search_module() -> Value {
    module(&[
        ("code", "search_code"),
        ("grep", "search_grep"),
        ("regex", "search_regex"),
        ("symbols", "search_symbols"),
        ("files", "search_files"),
        ("recent", "search_recent"),
        ("modified", "search_modified"),
        ("by_type", "search_by_type"),
        ("by_size", "search_by_size"),
        ("duplicates", "search_duplicates"),
        ("todos", "search_todos"),
        ("fixmes", "search_fixmes"),
        ("semantic", "search_semantic"),
        ("similar", "search_similar"),
    ])
}

/// Test execution module for AI coding assistants
pub fn test_module() -> Value {
    module(&[
        ("run", "test_run"),
        ("run_file", "test_run_file"),
        ("run_function", "test_run_function"),
        ("list", "test_list"),
        ("coverage", "test_coverage"),
        ("failing", "test_failing"),
        ("passing", "test_passing"),
        ("skipped", "test_skipped"),
        ("watch", "test_watch"),
        ("bench", "test_bench"),
        ("snapshot", "test_snapshot"),
        ("generate", "test_generate"),
    ])
}

/// Diagnostics module for AI coding assistants
pub fn diag_module() -> Value {
    module(&[
        ("check", "diag_check"),
        ("lint", "diag_lint"),
        ("errors", "diag_errors"),
        ("warnings", "diag_warnings"),
        ("all", "diag_all"),
        ("fix", "diag_fix"),
        ("explain", "diag_explain"),
        ("suppress", "diag_suppress"),
        ("config", "diag_config"),
    ])
}

/// Refactoring module for AI coding assistants
pub fn refactor_module() -> Value {
    module(&[
        ("rename", "refactor_rename"),
        ("rename_file", "refactor_rename_file"),
        ("extract_function", "refactor_extract_function"),
        ("extract_variable", "refactor_extract_variable"),
        ("extract_constant", "refactor_extract_constant"),
        ("inline", "refactor_inline"),
        ("move_to_file", "refactor_move_to_file"),
        ("change_signature", "refactor_change_signature"),
        ("add_parameter", "refactor_add_parameter"),
        ("remove_parameter", "refactor_remove_parameter"),
        ("organize_imports", "refactor_organize_imports"),
        ("remove_unused", "refactor_remove_unused"),
    ])
}

/// Session management module for AI coding assistants
pub fn session_module() -> Value {
    module(&[
        ("start", "session_start"),
        ("end", "session_end"),
        ("id", "session_id"),
        ("history", "session_history"),
        ("undo", "session_undo"),
        ("redo", "session_redo"),
        ("checkpoint", "session_checkpoint"),
        ("restore", "session_restore"),
        ("checkpoints", "session_checkpoints"),
        ("diff_since", "session_diff_since"),
        ("changes", "session_changes"),
        ("rollback", "session_rollback"),
        ("export", "session_export"),
    ])
}

/// Documentation module for AI coding assistants
pub fn docs_module() -> Value {
    module(&[
        ("generate", "docs_generate"),
        ("search", "docs_search"),
        ("api", "docs_api"),
        ("readme", "docs_readme"),
        ("changelog", "docs_changelog"),
        ("examples", "docs_examples"),
        ("types", "docs_types"),
        ("signatures", "docs_signatures"),
    ])
}

/// Environment detection module for AI coding assistants
pub fn devenv_module() -> Value {
    module(&[
        ("detect", "env_detect"),
        ("python", "env_python"),
        ("node", "env_node"),
        ("rust", "env_rust"),
        ("go", "env_go"),
        ("java", "env_java"),
        ("dotnet", "env_dotnet"),
        ("ruby", "env_ruby"),
        ("shell", "env_shell"),
        ("path", "env_path"),
        ("var", "env_var"),
        ("vars", "env_vars"),
        ("set", "env_set"),
        ("unset", "env_unset"),
        ("docker", "env_docker"),
        ("container", "env_container"),
        ("venv", "env_venv"),
        ("activate", "env_activate"),
    ])
}

/// Platform versioning and system snapshot module for cross-platform determinism
pub fn platform_module() -> Value {
    module(&[
        // System versioning
        ("snapshot", "platform_snapshot"),
        ("os_version", "platform_os_version"),
        ("kernel", "platform_kernel"),
        ("build", "platform_build"),
        ("arch", "platform_arch"),
        ("hostname", "platform_hostname"),
        ("machine_id", "platform_machine_id"),
        // Tool versioning
        ("tool_version", "platform_tool_version"),
        ("tool_versions", "platform_tool_versions"),
        ("detect_tools", "platform_detect_tools"),
        ("has_tool", "platform_has_tool"),
        // Capabilities
        ("capabilities", "platform_capabilities"),
        ("has_admin", "platform_has_admin"),
        ("has_sudo", "platform_has_sudo"),
        ("has_gui", "platform_has_gui"),
        ("has_network", "platform_has_network"),
        ("has_container", "platform_has_container"),
        ("pkg_managers", "platform_pkg_managers"),
        // Normalization
        ("normalize_path", "platform_normalize_path"),
        ("normalize_output", "platform_normalize_output"),
        ("path_sep", "platform_path_sep"),
        ("line_ending", "platform_line_ending"),
        ("shell_type", "platform_shell_type"),
        // Requirements
        ("require", "platform_require"),
        ("check_reqs", "platform_check_reqs"),
        ("satisfies", "platform_satisfies"),
        // Diff/compare
        ("diff", "platform_diff"),
        ("compatible", "platform_compatible"),
        // Database operations
        ("db_init", "platform_db_init"),
        ("db_store", "platform_db_store"),
        ("db_load", "platform_db_load"),
        ("db_list", "platform_db_list"),
        ("db_delete", "platform_db_delete"),
        ("db_compare", "platform_db_compare"),
        ("db_export", "platform_db_export"),
        ("db_import", "platform_db_import"),
        // Fingerprinting
        ("fingerprint", "platform_fingerprint"),
        ("hash", "platform_hash"),
        // Categorized tool queries
        ("compilers", "platform_compilers"),
        ("build_systems", "platform_build_systems"),
        ("runtimes", "platform_runtimes"),
        ("pkg_lang", "platform_pkg_lang"),
        ("containers", "platform_containers"),
        ("cloud_clis", "platform_cloud_clis"),
        ("databases", "platform_databases"),
        ("linters", "platform_linters"),
        ("vcs", "platform_vcs"),
        ("iac_tools", "platform_iac_tools"),
        // Library detection
        ("libs", "platform_libs"),
        ("lib_version", "platform_lib_version"),
        ("system_libs", "platform_system_libs"),
        ("sdk_version", "platform_sdk_version"),
        // Runtime info
        ("libc", "platform_libc"),
        ("libcpp", "platform_libcpp"),
        ("ssl_version", "platform_ssl_version"),
        ("cuda_version", "platform_cuda_version"),
        // Hardware detection
        ("cpu", "platform_cpu"),
        ("cpu_count", "platform_cpu_count"),
        ("cpu_freq", "platform_cpu_freq"),
        ("memory", "platform_memory"),
        ("memory_total", "platform_memory_total"),
        ("memory_free", "platform_memory_free"),
        ("disks", "platform_disks"),
        ("disk_usage", "platform_disk_usage"),
        ("gpus", "platform_gpus"),
        ("gpu_memory", "platform_gpu_memory"),
        ("network_interfaces", "platform_network_interfaces"),
        ("hardware_summary", "platform_hardware_summary"),
    ])
}

/// Get all modules as a map for registering in the environment

/// Monitor module: monitor.health(), monitor.htop(), monitor.alert_create(), etc.
pub fn monitor_module() -> Value {
    module(&[
        // System monitoring
        ("htop", "monitor_htop"),
        ("iotop", "monitor_iotop"),
        ("nmon", "monitor_nmon"),
        ("nethogs", "monitor_nethogs"),
        ("iftop", "monitor_iftop"),
        ("glances", "monitor_glances"),
        ("tcpdump", "monitor_tcpdump"),
        ("sockets", "monitor_sockets"),
        ("ip_addr", "monitor_ip_addr"),
        ("ip_route", "monitor_ip_route"),
        ("ip_link", "monitor_ip_link"),
        ("ethtool", "monitor_ethtool"),
        ("perf_stat", "monitor_perf_stat"),
        ("perf_record", "monitor_perf_record"),
        ("memory", "monitor_memory"),
        ("uptime", "monitor_uptime"),
        ("users", "monitor_users"),
        ("logins", "monitor_logins"),
        ("syslog", "monitor_syslog"),
        ("dstat", "monitor_dstat"),
        // Proactive alerting
        ("health", "monitor_health_check"),
        ("alert_create", "monitor_alert_create"),
        ("alert_list", "monitor_alert_list"),
        ("alert_history", "monitor_alert_history"),
        ("alert_remove", "monitor_alert_remove"),
        ("watch_cpu", "monitor_watch_cpu"),
        ("watch_memory", "monitor_watch_memory"),
        ("watch_disk", "monitor_watch_disk"),
    ])
}

// ═══════════════════════════════════════════════════════════════════════════
// CLI Tool Wrapper Modules
// ═══════════════════════════════════════════════════════════════════════════

pub fn ssh_module() -> Value {
    module(&[
        ("exec", "ssh_exec"),
        ("tunnel", "ssh_tunnel"),
        ("keygen", "ssh_keygen"),
        ("copy_id", "ssh_copy_id"),
        ("config", "ssh_config"),
    ])
}

pub fn rsync_module() -> Value {
    module(&[("sync", "rsync_sync")])
}

pub fn scp_module() -> Value {
    module(&[("upload", "scp_upload"), ("download", "scp_download")])
}

pub fn openssl_module() -> Value {
    module(&[
        ("cert_info", "openssl_cert_info"),
        ("genrsa", "openssl_genrsa"),
    ])
}

pub fn gpg_module() -> Value {
    module(&[
        ("list_keys", "gpg_list_keys"),
        ("encrypt", "gpg_encrypt"),
        ("decrypt", "gpg_decrypt"),
    ])
}

pub fn age_module() -> Value {
    module(&[
        ("encrypt", "age_encrypt"),
        ("decrypt", "age_decrypt"),
        ("keygen", "age_keygen"),
    ])
}

pub fn docker_module() -> Value {
    module(&[
        ("ps", "docker_ps"),
        ("images", "docker_images"),
        ("run", "docker_run"),
        ("exec", "docker_exec"),
        ("logs", "docker_logs"),
        ("stop", "docker_stop"),
        ("rm", "docker_rm"),
        ("build", "docker_build"),
        ("pull", "docker_pull"),
        ("push", "docker_push"),
        ("inspect", "docker_inspect"),
        ("volumes", "docker_volumes"),
        ("networks", "docker_networks"),
        ("compose_up", "docker_compose_up"),
        ("compose_down", "docker_compose_down"),
        ("compose_ps", "docker_compose_ps"),
        ("stats", "docker_stats"),
        ("top", "docker_top"),
        ("cp", "docker_cp"),
        ("tag", "docker_tag"),
    ])
}

pub fn podman_module() -> Value {
    module(&[
        ("ps", "podman_ps"),
        ("images", "podman_images"),
        ("run", "podman_run"),
        ("exec", "podman_exec"),
        ("logs", "podman_logs"),
        ("stop", "podman_stop"),
        ("rm", "podman_rm"),
        ("build", "podman_build"),
        ("pull", "podman_pull"),
    ])
}

pub fn k8s_module() -> Value {
    module(&[
        ("get", "k8s_get"),
        ("apply", "k8s_apply"),
        ("delete", "k8s_delete"),
        ("describe", "k8s_describe"),
        ("logs", "k8s_logs"),
        ("exec", "k8s_exec"),
        ("pods", "k8s_pods"),
        ("services", "k8s_services"),
        ("deployments", "k8s_deployments"),
        ("namespaces", "k8s_namespaces"),
        ("nodes", "k8s_nodes"),
        ("events", "k8s_events"),
        ("configmaps", "k8s_configmaps"),
        ("secrets", "k8s_secrets"),
        ("ingresses", "k8s_ingresses"),
        ("rollout_status", "k8s_rollout_status"),
        ("rollout_restart", "k8s_rollout_restart"),
        ("scale", "k8s_scale"),
        ("top_pods", "k8s_top_pods"),
        ("top_nodes", "k8s_top_nodes"),
        ("context", "k8s_context"),
        ("contexts", "k8s_contexts"),
        ("cluster_info", "k8s_cluster_info"),
    ])
}

pub fn helm_module() -> Value {
    module(&[
        ("list", "helm_list"),
        ("install", "helm_install"),
        ("upgrade", "helm_upgrade"),
        ("uninstall", "helm_uninstall"),
        ("repos", "helm_repos"),
        ("search", "helm_search"),
        ("status", "helm_status"),
    ])
}

pub fn terraform_module() -> Value {
    module(&[
        ("init", "terraform_init"),
        ("plan", "terraform_plan"),
        ("apply", "terraform_apply"),
        ("destroy", "terraform_destroy"),
        ("state", "terraform_state"),
        ("output", "terraform_output"),
        ("workspace", "terraform_workspace"),
        ("validate", "terraform_validate"),
    ])
}

pub fn ansible_module() -> Value {
    module(&[
        ("playbook", "ansible_playbook"),
        ("inventory", "ansible_inventory"),
        ("galaxy", "ansible_galaxy"),
        ("vault", "ansible_vault"),
    ])
}

pub fn rg_module() -> Value {
    module(&[("search", "rg_search")])
}

pub fn jq_module() -> Value {
    module(&[("query", "jq_query"), ("filter", "jq_filter")])
}

pub fn yq_module() -> Value {
    module(&[("query", "yq_query")])
}

pub fn make_module() -> Value {
    module(&[("run", "make_run"), ("targets", "make_targets")])
}

pub fn cmake_module() -> Value {
    module(&[("configure", "cmake_configure"), ("build", "cmake_build")])
}

pub fn ninja_module() -> Value {
    module(&[("build", "ninja_build")])
}

pub fn nmap_module() -> Value {
    module(&[("scan", "nmap_scan"), ("quick", "nmap_quick")])
}

// ============================================================
// CLI Tool Wrapper Modules — Tools 51-150
// ============================================================

pub fn tmux_module() -> Value {
    module(&[
        ("new", "tmux_new"),
        ("list", "tmux_list"),
        ("attach", "tmux_attach"),
        ("send", "tmux_send"),
    ])
}

pub fn screen_module() -> Value {
    module(&[
        ("new", "screen_new"),
        ("list", "screen_list"),
        ("attach", "screen_attach"),
    ])
}

pub fn valgrind_module() -> Value {
    module(&[("run", "valgrind_run"), ("memcheck", "valgrind_memcheck")])
}

pub fn gdb_module() -> Value {
    module(&[("run", "gdb_run"), ("bt", "gdb_bt")])
}

pub fn objdump_module() -> Value {
    module(&[("disasm", "objdump_disasm"), ("headers", "objdump_headers")])
}

pub fn readelf_module() -> Value {
    module(&[
        ("headers", "readelf_headers"),
        ("sections", "readelf_sections"),
    ])
}

pub fn zoxide_module() -> Value {
    module(&[("add", "zoxide_add"), ("query", "zoxide_query")])
}

pub fn just_module() -> Value {
    module(&[("run", "just_run"), ("list", "just_list")])
}

pub fn taskfile_module() -> Value {
    module(&[("run", "task_run"), ("list", "task_list")])
}

pub fn direnv_module() -> Value {
    module(&[("allow", "direnv_allow"), ("status", "direnv_status")])
}

pub fn asdf_module() -> Value {
    module(&[("list", "asdf_list"), ("install", "asdf_install")])
}

pub fn mise_module() -> Value {
    module(&[("use_version", "mise_use"), ("list", "mise_list")])
}

pub fn uv_module() -> Value {
    module(&[("run", "uv_run"), ("pip", "uv_pip"), ("venv", "uv_venv")])
}

pub fn pipx_module() -> Value {
    module(&[("install", "pipx_install"), ("list", "pipx_list")])
}

pub fn poetry_module() -> Value {
    module(&[("run", "poetry_run"), ("install", "poetry_install")])
}

pub fn node_module() -> Value {
    module(&[("run", "node_run")])
}

pub fn npm_module() -> Value {
    module(&[("run", "npm_run"), ("install", "npm_install")])
}

pub fn pnpm_module() -> Value {
    module(&[("run", "pnpm_run"), ("install", "pnpm_install")])
}

pub fn yarn_module() -> Value {
    module(&[("run", "yarn_run"), ("install", "yarn_install")])
}

pub fn cargo_module() -> Value {
    module(&[
        ("run", "cargo_run"),
        ("build", "cargo_build_cmd"),
        ("test", "cargo_test_cmd"),
    ])
}

pub fn rustup_module() -> Value {
    module(&[("show", "rustup_show"), ("update", "rustup_update")])
}

pub fn go_module() -> Value {
    module(&[
        ("run", "go_run"),
        ("build", "go_build"),
        ("test", "go_test"),
    ])
}

pub fn bun_module() -> Value {
    module(&[("run", "bun_run"), ("install", "bun_install")])
}

pub fn deno_module() -> Value {
    module(&[("run", "deno_run"), ("task", "deno_task")])
}

pub fn gh_module() -> Value {
    module(&[
        ("pr", "gh_pr"),
        ("issue", "gh_issue"),
        ("repo", "gh_repo_cmd"),
    ])
}

pub fn glab_module() -> Value {
    module(&[("mr", "glab_mr"), ("issue", "glab_issue")])
}

pub fn pre_commit_module() -> Value {
    module(&[("run", "pre_commit_run"), ("install", "pre_commit_install")])
}

pub fn buildah_module() -> Value {
    module(&[("build", "buildah_build"), ("images", "buildah_images")])
}

pub fn skopeo_module() -> Value {
    module(&[("copy", "skopeo_copy"), ("inspect", "skopeo_inspect")])
}

pub fn trivy_module() -> Value {
    module(&[("scan", "trivy_scan"), ("image", "trivy_image")])
}

pub fn ruff_module() -> Value {
    module(&[("check", "ruff_check"), ("format", "ruff_format")])
}

pub fn iperf3_module() -> Value {
    module(&[("server", "iperf3_server"), ("client", "iperf3_client")])
}

pub fn nc_module() -> Value {
    module(&[("connect", "nc_connect"), ("listen", "nc_listen")])
}

pub fn all_modules() -> Vec<(&'static str, Value)> {
    vec![
        ("sys", sys_module()),
        ("proc", proc_module()),
        ("fs", fs_module()),
        ("file", file_module()),
        ("net", net_module()),
        ("http", http_module()),
        ("gui", gui_module()),
        ("web", web_module()),
        ("crypto", crypto_module()),
        ("db", db_module()),
        ("svc", svc_module()),
        ("cron", cron_module()),
        ("archive", archive_module()),
        ("user", user_module()),
        ("perm", perm_module()),
        ("pkg", pkg_module()),
        ("hw", hw_module()),
        ("clip", clip_module()),
        ("input", input_module()),
        ("ai", ai_module()),
        ("agent", agent_module()),
        ("math", math_module()),
        ("str", str_module()),
        ("arr", arr_module()),
        ("json", json_module()),
        ("mcp", mcp_module()),
        ("shell", shell_module()),
        ("a2ui", a2ui_module()),
        ("a2a", a2a_module()),
        ("nanda", nanda_module()),
        ("git", git_module()),
        ("code", code_module()),
        ("project", project_module()),
        ("search", search_module()),
        ("test", test_module()),
        ("diag", diag_module()),
        ("refactor", refactor_module()),
        ("session", session_module()),
        ("docs", docs_module()),
        ("devenv", devenv_module()),
        ("platform", platform_module()),
        ("rbac", rbac_module()),
        ("audit", audit_module()),
        ("sso", sso_module()),
        ("cluster", cluster_module()),
        ("nn", nn_module()),
        ("evo", evo_module()),
        ("rl", rl_module()),
        ("monitor", monitor_module()),
        ("cloud", cloud_module()),
        ("repl", repl_module()),
        ("workspace", workspace_module()),
        ("marketplace", marketplace_module()),
        ("telemetry", telemetry_module()),
        // ── CLI Tool Wrapper Modules ────────────────────────────
        ("ssh", ssh_module()),
        ("rsync", rsync_module()),
        ("scp", scp_module()),
        ("openssl", openssl_module()),
        ("gpg", gpg_module()),
        ("age", age_module()),
        ("docker", docker_module()),
        ("podman", podman_module()),
        ("k8s", k8s_module()),
        ("kubectl", k8s_module()), // alias
        ("helm", helm_module()),
        ("terraform", terraform_module()),
        ("ansible", ansible_module()),
        ("rg", rg_module()),
        ("jq", jq_module()),
        ("yq", yq_module()),
        ("make", make_module()),
        ("cmake", cmake_module()),
        ("ninja", ninja_module()),
        ("nmap", nmap_module()),
        // ── CLI Tool Wrapper Modules (Tools 51-150) ─────────────
        ("tmux", tmux_module()),
        ("screen", screen_module()),
        ("valgrind", valgrind_module()),
        ("gdb", gdb_module()),
        ("objdump", objdump_module()),
        ("readelf", readelf_module()),
        ("zoxide", zoxide_module()),
        ("just", just_module()),
        ("taskfile", taskfile_module()),
        ("direnv", direnv_module()),
        ("asdf", asdf_module()),
        ("mise", mise_module()),
        ("uv", uv_module()),
        ("pipx", pipx_module()),
        ("poetry", poetry_module()),
        ("node", node_module()),
        ("npm", npm_module()),
        ("pnpm", pnpm_module()),
        ("yarn", yarn_module()),
        ("cargo", cargo_module()),
        ("rustup", rustup_module()),
        ("go", go_module()),
        ("bun", bun_module()),
        ("deno", deno_module()),
        ("gh", gh_module()),
        ("glab", glab_module()),
        ("pre_commit", pre_commit_module()),
        ("buildah", buildah_module()),
        ("skopeo", skopeo_module()),
        ("trivy", trivy_module()),
        ("ruff", ruff_module()),
        ("iperf3", iperf3_module()),
        ("nc", nc_module()),
        ("netcat", nc_module()),
    ]
}
