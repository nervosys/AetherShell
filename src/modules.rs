//! Module system for AetherShell builtins.
//!
//! This module provides namespaced access to builtin functions:
//! - `sys.info()`, `sys.hostname()`, `sys.uptime()`
//! - `net.interfaces()`, `net.ping()`, `net.dns_lookup()`
//! - `gui.screenshot()`, `gui.click()`, `gui.type_text()`
//! - `crypto.hash()`, `crypto.encrypt()`, `crypto.sign()`
//! - etc.

use std::collections::BTreeMap;
use crate::value::{Value, BuiltinRef};

/// Create a builtin reference value
pub fn builtin_ref(name: &str) -> Value {
    Value::Builtin(BuiltinRef { name: name.to_string() })
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

/// Get all modules as a map for registering in the environment
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
        ("math", math_module()),
        ("str", str_module()),
        ("arr", arr_module()),
        ("json", json_module()),
        ("mcp", mcp_module()),
        ("shell", shell_module()),
        ("a2ui", a2ui_module()),
        ("cluster", cluster_module()),
        ("nn", nn_module()),
        ("evo", evo_module()),
        ("rl", rl_module()),
    ]
}
