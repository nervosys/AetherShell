// src/builtins.rs
use anyhow::{anyhow, Context, Result};
use serde_json;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use walkdir::WalkDir;

use crate::{
    config::{get_config, reload_config, ShellConfig},
    env::Env,
    eval::eval_expr,
    evolution::{CrossoverStrategy, EvolutionConfig, FitnessResult, Population, SelectionStrategy},
    neural::{Activation, ConsensusNetwork, NeuralNetwork},
    os_tools::{execute_tool_safe, OSToolsDatabase, OperatingSystem, ToolCategory},
    rl::{
        ActorCriticAgent, DQNAgent, GridWorld, PolicyGradientAgent, QLearningAgent, ReplayBuffer,
        SarsaAgent,
    },
    rlm::{run_recursive, run_recursive_with_model, RlmConfig},
    security::{
        check_file_size_limit, check_rate_limit, create_secure_http_client, validate_ai_prompt,
        validate_http_url, validate_read_path,
    },
    shell_features::{Parameter, ParameterSet, ParameterType, PipelineInputType},
    value::{Lambda, Value},
};
use std::time::Duration;

// Fast builtin lookup using hash map
lazy_static::lazy_static! {
    static ref BUILTIN_LOOKUP: HashMap<&'static str, usize> = {
        let mut map = HashMap::new();

        // General functions
        map.insert("help", 0);
        map.insert("call", 1);
        map.insert("clear", 2);
        map.insert("echo", 3);
        map.insert("print", 4);
        map.insert("http_get", 5);

        // Option constructors
        map.insert("Some", 6);
        map.insert("None", 7);

        // File system
        map.insert("ls", 8);
        map.insert("list", 8); // alias
        map.insert("pwd", 9);
        map.insert("cat", 10);
        map.insert("read_text", 11);
        map.insert("head", 12);
        map.insert("tail", 13);
        map.insert("find", 14);
        map.insert("sort", 15);
        map.insert("uniq", 16);
        map.insert("wc", 17);
        map.insert("grep", 18);

        // Data/pipelines
        map.insert("map", 19);
        map.insert("where", 20);
        map.insert("reduce", 21);
        map.insert("take", 22);
        map.insert("keys", 23);
        map.insert("len", 24);
        map.insert("length", 24); // alias
        map.insert("type_of", 25);
        map.insert("typeof", 25); // alias

        // Array functions
        map.insert("first", 26);
        map.insert("last", 27);
        map.insert("any", 28);
        map.insert("all", 29);

        // MCP functions
        map.insert("mcp_servers", 30);
        map.insert("mcp-servers", 30); // alias
        map.insert("mcp_detect", 31);
        map.insert("mcp-detect", 31); // alias
        map.insert("mcp_cache_clear", 32);
        map.insert("mcp-cache-clear", 32); // alias
        map.insert("mcp_cache_status", 33);
        map.insert("mcp-cache-status", 33); // alias

        // String functions
        map.insert("split", 34);
        map.insert("join", 35);
        map.insert("trim", 36);
        map.insert("upper", 37);
        map.insert("lower", 38);
        map.insert("replace", 39);
        map.insert("contains", 40);
        map.insert("starts_with", 41);
        map.insert("ends_with", 42);

        // Array functions (extended)
        map.insert("flatten", 43);
        map.insert("reverse", 44);
        map.insert("slice", 45);
        map.insert("range", 46);
        map.insert("zip", 47);
        map.insert("push", 48);
        map.insert("concat", 49);

        // Math functions
        map.insert("abs", 50);
        map.insert("min", 51);
        map.insert("max", 52);
        map.insert("floor", 53);
        map.insert("ceil", 54);
        map.insert("round", 55);
        map.insert("sqrt", 56);
        map.insert("pow", 57);

        // Utility functions
        map.insert("exit", 58);
        map.insert("env", 59);
        map.insert("set_env", 60);
        map.insert("sleep", 61);
        map.insert("time", 62);
        map.insert("json_parse", 63);
        map.insert("json_stringify", 64);

        // Syntax Knowledge Base functions
        map.insert("syntax_get", 65);
        map.insert("syntax_list", 66);
        map.insert("syntax_search", 67);
        map.insert("syntax_add", 68);
        map.insert("syntax_categories", 69);
        map.insert("ab_encode", 70);
        map.insert("ab_decode", 71);

        // Neural Network functions (72-82)
        map.insert("nn_create", 72);
        map.insert("nn_forward", 73);
        map.insert("nn_mutate", 74);
        map.insert("nn_crossover", 75);
        map.insert("nn_params", 76);
        map.insert("nn_set_params", 77);
        map.insert("nn_layers", 78);
        map.insert("nn_info", 79);
        map.insert("consensus_net", 80);
        map.insert("consensus_vote", 81);
        map.insert("activation", 82);

        // Evolution functions (83-92)
        map.insert("population", 83);
        map.insert("evolve", 84);
        map.insert("evolve_step", 85);
        map.insert("fitness", 86);
        map.insert("best_individual", 87);
        map.insert("evolution_stats", 88);
        map.insert("selection_strategy", 89);
        map.insert("crossover_strategy", 90);
        map.insert("evolution_config", 91);
        map.insert("coevolve", 92);

        // Reinforcement Learning functions (93-107)
        map.insert("rl_agent", 93);
        map.insert("rl_action", 94);
        map.insert("rl_update", 95);
        map.insert("rl_sarsa_agent", 96);
        map.insert("rl_sarsa_update", 97);
        map.insert("rl_pg_agent", 98);
        map.insert("rl_pg_step", 99);
        map.insert("rl_pg_episode_end", 100);
        map.insert("rl_ac_agent", 101);
        map.insert("rl_ac_update", 102);
        map.insert("rl_dqn_agent", 103);
        map.insert("rl_dqn_step", 104);
        map.insert("rl_replay_buffer", 105);
        map.insert("rl_gridworld", 106);
        map.insert("rl_env_step", 107);

        // OS Tools functions (108-112)
        map.insert("tools", 108);
        map.insert("tool_list", 108); // alias
        map.insert("os_tools", 108); // alias for tools
        map.insert("tool_info", 109);
        map.insert("tool_schema", 110);
        map.insert("tool_schemas", 110); // alias
        map.insert("tool_exec", 111);
        map.insert("tool_execute", 111); // alias
        map.insert("tool_search", 112);

        // Recursive Language Model functions (113-116)
        map.insert("rlm_agent", 113);
        map.insert("recursive_agent", 113); // alias
        map.insert("rlm_config", 114);
        map.insert("rlm_stats", 115);
        map.insert("rlm_spawn", 116);
        map.insert("spawn_agent", 116); // alias

        // MCP Server/Client functions (117-122)
        map.insert("mcp_server", 117);
        map.insert("mcp_tools", 118);
        map.insert("mcp_list_tools", 118); // alias
        map.insert("mcp_call", 119);
        map.insert("mcp_call_tool", 119); // alias
        map.insert("mcp_resources", 120);
        map.insert("mcp_list_resources", 120); // alias
        map.insert("mcp_prompts", 121);
        map.insert("mcp_list_prompts", 121); // alias
        map.insert("mcp_connect", 122);
        map.insert("mcp_client", 122); // alias

        // New aspirational features (123-131)
        map.insert("sh", 123);
        map.insert("shell", 123); // alias
        map.insert("now", 124);
        map.insert("timestamp", 124); // alias
        map.insert("sort_by", 125);
        map.insert("save_json", 126);
        map.insert("write_json", 126); // alias
        map.insert("ai_backends", 127);
        map.insert("mcp_server_start", 128);
        map.insert("agent_with_mcp", 129);
        map.insert("each", 130); // alias for map with side effects
        map.insert("in", 131); // membership test

        // Additional utility functions (132-136)
        map.insert("values", 132);
        map.insert("sum", 133);
        map.insert("unique", 134);
        map.insert("avg", 135);
        map.insert("mean", 135); // alias for avg
        map.insert("product", 136);

        // Configuration functions (137-143)
        map.insert("config", 137);
        map.insert("config_get", 138);
        map.insert("config_set", 139);
        map.insert("config_path", 140);
        map.insert("config_init", 141);
        map.insert("config_reload", 142);
        map.insert("themes", 143);

        // Plugin functions (144-150)
        map.insert("plugins", 144);
        map.insert("plugin_list", 144); // alias
        map.insert("plugin_info", 145);
        map.insert("plugin_enable", 146);
        map.insert("plugin_disable", 147);
        map.insert("plugin_load", 148);
        map.insert("plugin_unload", 149);
        map.insert("plugin_categories", 150);

        map
    };
}

// Fast dispatch table for builtin functions
static BUILTIN_DISPATCH: &[fn(Vec<Value>, Option<Value>, &mut Env) -> Result<Value>] = &[
    // 0-9: General and file system functions
    |_, _, _| bi_help(),
    |args, input, env| bi_call(args, input, env),
    |_, _, _| bi_clear(),
    |args, _, _| bi_echo(&args),
    |args, input, _| bi_print(args, input),
    |args, input, _| bi_http_get(args, input),
    |args, _, _| bi_some(args),
    |_, _, _| bi_none(),
    |args, input, _| bi_ls(args, input),
    |_, _, _| bi_pwd(),
    // 10-19: File operations and data functions
    |args, input, _| bi_cat(args, input),
    |args, input, _| bi_read_text(args, input),
    |args, input, _| bi_head(args, input),
    |args, input, _| bi_tail(args, input),
    |args, input, _| bi_find(args, input),
    |args, input, _| bi_sort(args, input),
    |args, input, _| bi_uniq(args, input),
    |args, input, _| bi_wc(args, input),
    |args, input, _| bi_grep(args, input),
    |args, input, env| bi_map(args, input, env),
    // 20-29: Pipeline and array functions
    |args, input, env| bi_where(args, input, env),
    |args, input, env| bi_reduce(args, input, env),
    |args, input, _| bi_take(args, input),
    |args, input, _| bi_keys(args, input),
    |args, input, _| bi_len(args, input),
    |args, input, _| bi_type_of(args, input),
    |args, input, _| bi_first(args, input),
    |args, input, _| bi_last(args, input),
    |args, input, env| bi_any(args, input, env),
    |args, input, env| bi_all(args, input, env),
    // 30-33: MCP functions
    |args, input, _| bi_mcp_servers(args, input),
    |args, input, _| bi_mcp_detect(args, input),
    |args, input, _| bi_mcp_cache_clear(args, input),
    |args, input, _| bi_mcp_cache_status(args, input),
    // 34-42: String functions
    |args, input, _| bi_split(args, input),
    |args, input, _| bi_join(args, input),
    |args, input, _| bi_trim(args, input),
    |args, input, _| bi_upper(args, input),
    |args, input, _| bi_lower(args, input),
    |args, input, _| bi_replace(args, input),
    |args, input, _| bi_contains(args, input),
    |args, input, _| bi_starts_with(args, input),
    |args, input, _| bi_ends_with(args, input),
    // 43-49: Array functions (extended)
    |args, input, _| bi_flatten(args, input),
    |args, input, _| bi_reverse(args, input),
    |args, input, _| bi_slice(args, input),
    |args, input, _| bi_range(args, input),
    |args, input, _| bi_zip(args, input),
    |args, input, _| bi_push(args, input),
    |args, input, _| bi_concat(args, input),
    // 50-57: Math functions
    |args, input, _| bi_abs(args, input),
    |args, input, _| bi_min(args, input),
    |args, input, _| bi_max(args, input),
    |args, input, _| bi_floor(args, input),
    |args, input, _| bi_ceil(args, input),
    |args, input, _| bi_round(args, input),
    |args, input, _| bi_sqrt(args, input),
    |args, input, _| bi_pow(args, input),
    // 58-64: Utility functions
    |args, input, _| bi_exit(args, input),
    |args, input, _| bi_env(args, input),
    |args, input, _| bi_set_env(args, input),
    |args, input, _| bi_sleep(args, input),
    |args, input, _| bi_time(args, input),
    |args, input, _| bi_json_parse(args, input),
    |args, input, _| bi_json_stringify(args, input),
    // 65-71: Syntax Knowledge Base functions
    |args, input, _| bi_syntax_get(args, input),
    |args, input, _| bi_syntax_list(args, input),
    |args, input, _| bi_syntax_search(args, input),
    |args, input, _| bi_syntax_add(args, input),
    |args, input, _| bi_syntax_categories(args, input),
    |args, input, _| bi_ab_encode(args, input),
    |args, input, _| bi_ab_decode(args, input),
    // 72-82: Neural Network functions
    |args, input, _| bi_nn_create(args, input),
    |args, input, _| bi_nn_forward(args, input),
    |args, input, _| bi_nn_mutate(args, input),
    |args, input, _| bi_nn_crossover(args, input),
    |args, input, _| bi_nn_params(args, input),
    |args, input, _| bi_nn_set_params(args, input),
    |args, input, _| bi_nn_layers(args, input),
    |args, input, _| bi_nn_info(args, input),
    |args, input, _| bi_consensus_net(args, input),
    |args, input, _| bi_consensus_vote(args, input),
    |args, input, _| bi_activation(args, input),
    // 83-92: Evolution functions
    |args, input, env| bi_population(args, input, env),
    |args, input, env| bi_evolve(args, input, env),
    |args, input, env| bi_evolve_step(args, input, env),
    |args, input, env| bi_fitness(args, input, env),
    |args, input, _| bi_best_individual(args, input),
    |args, input, _| bi_evolution_stats(args, input),
    |args, input, _| bi_selection_strategy(args, input),
    |args, input, _| bi_crossover_strategy(args, input),
    |args, input, _| bi_evolution_config(args, input),
    |args, input, env| bi_coevolve(args, input, env),
    // 93-107: Reinforcement Learning functions
    |args, input, _| bi_rl_agent(args, input),
    |args, input, _| bi_rl_action(args, input),
    |args, input, _| bi_rl_update(args, input),
    |args, input, _| bi_rl_sarsa_agent(args, input),
    |args, input, _| bi_rl_sarsa_update(args, input),
    |args, input, _| bi_rl_pg_agent(args, input),
    |args, input, _| bi_rl_pg_step(args, input),
    |args, input, _| bi_rl_pg_episode_end(args, input),
    |args, input, _| bi_rl_ac_agent(args, input),
    |args, input, _| bi_rl_ac_update(args, input),
    |args, input, _| bi_rl_dqn_agent(args, input),
    |args, input, _| bi_rl_dqn_step(args, input),
    |args, input, _| bi_rl_replay_buffer(args, input),
    |args, input, _| bi_rl_gridworld(args, input),
    |args, input, _| bi_rl_env_step(args, input),
    // 108-112: OS Tools functions
    |args, input, _| bi_tools(args, input),
    |args, input, _| bi_tool_info(args, input),
    |args, input, _| bi_tool_schema(args, input),
    |args, input, _| bi_tool_exec(args, input),
    |args, input, _| bi_tool_search(args, input),
    // 113-116: Recursive Language Model functions
    |args, input, env| bi_rlm_agent(args, input, env),
    |args, input, _| bi_rlm_config(args, input),
    |args, input, _| bi_rlm_stats(args, input),
    |args, input, env| bi_rlm_spawn(args, input, env),
    // 117-122: MCP Server/Client functions
    |args, input, _| bi_mcp_server(args, input),
    |args, input, _| bi_mcp_tools(args, input),
    |args, input, _| bi_mcp_call(args, input),
    |args, input, _| bi_mcp_resources(args, input),
    |args, input, _| bi_mcp_prompts(args, input),
    |args, input, _| bi_mcp_connect(args, input),
    // 123-131: New aspirational features
    |args, input, _| bi_sh(args, input),
    |args, input, _| bi_now(args, input),
    |args, input, env| bi_sort_by(args, input, env),
    |args, input, _| bi_save_json(args, input),
    |args, input, _| bi_ai_backends(args, input),
    |args, input, _| bi_mcp_server_start(args, input),
    |args, input, env| bi_agent_with_mcp(args, input, env),
    |args, input, env| bi_each(args, input, env),
    |args, input, _| bi_in(args, input),
    // 132-136: Additional utility functions
    |args, input, _| bi_values(args, input),
    |args, input, _| bi_sum(args, input),
    |args, input, _| bi_unique(args, input),
    |args, input, _| bi_avg(args, input),
    |args, input, _| bi_product(args, input),
    // 137-142: Configuration functions
    |args, input, _| bi_config(args, input),
    |args, input, _| bi_config_get(args, input),
    |args, input, _| bi_config_set(args, input),
    |_, _, _| bi_config_path(),
    |_, _, _| bi_config_init(),
    |_, _, _| bi_config_reload(),
    |_, _, _| bi_themes(),
    // 144-150: Plugin functions
    |args, input, _| bi_plugins(args, input),
    |args, input, _| bi_plugin_info(args, input),
    |args, input, _| bi_plugin_enable(args, input),
    |args, input, _| bi_plugin_disable(args, input),
    |args, input, _| bi_plugin_load(args, input),
    |args, input, _| bi_plugin_unload(args, input),
    |_, _, _| bi_plugin_categories(),
];

fn fast_builtin_lookup(
    name: &str,
    args: Vec<Value>,
    input: Option<Value>,
    env: &mut Env,
) -> Option<Result<Value>> {
    if let Some(&index) = BUILTIN_LOOKUP.get(name) {
        if index < BUILTIN_DISPATCH.len() {
            Some(BUILTIN_DISPATCH[index](args, input, env))
        } else {
            None
        }
    } else {
        None
    }
}

// --------------- Public entry points ---------------

pub fn call(name: &str, args: Vec<Value>, env: &mut Env) -> Result<Value> {
    call_with_input(name, args, None, env)
}
/// Call a builtin with optional piped input
pub fn call_with_input(
    name: &str,
    args: Vec<Value>,
    input: Option<Value>,
    env: &mut Env,
) -> Result<Value> {
    // Try fast lookup first
    if let Some(result) = fast_builtin_lookup(name, args.clone(), input.clone(), env) {
        return result;
    }

    // Fall back to the comprehensive match for functions not in fast lookup
    match name {
        // PowerShell-style cmdlets (not in fast lookup)
        "Get-Files" | "get-files" => bi_get_files(args, input),
        "Get-Content" | "get-content" => bi_get_content(args, input),
        "Select-Object" | "select-object" | "select" => bi_select_object(args, input),
        "Where-Object" | "where-object" => bi_where_object(args, input, env),
        "ForEach-Object" | "foreach-object" | "foreach" => bi_foreach_object(args, input, env),
        "Sort-Object" | "sort-object" => bi_sort_object(args, input),
        "Group-Object" | "group-object" | "group" | "group_by" => bi_group_object(args, input),
        "Measure-Object" | "measure-object" | "measure" => bi_measure_object(args, input),

        // Nushell-style data commands (not in fast lookup)
        "from-json" | "from_json" => bi_from_json(args, input),
        "to-json" | "to_json" => bi_to_json(args, input),
        "from-csv" | "from_csv" => bi_from_csv(args, input),
        "to-csv" | "to_csv" => bi_to_csv(args, input),
        "from-yaml" | "from_yaml" => bi_from_yaml(args, input),
        "to-yaml" | "to_yaml" => bi_to_yaml(args, input),
        "columns" => bi_columns(args, input),
        "describe" => bi_describe(args, input),

        // AI-enhanced commands (not in fast lookup)
        "ai" => bi_ai(args, input, env),
        "ai-suggest" | "suggest" => bi_ai_suggest(args, input, env),
        "ai-explain" | "explain" => bi_ai_explain(args, input, env),
        "ai-complete" | "complete" => bi_ai_complete(args, input, env),
        "ai-fix" | "fix" => bi_ai_fix(args, input, env),

        // AI/Agents (not in fast lookup)
        "agent" => bi_agent(args, input, env),
        "swarm" => bi_swarm(args, input, env),

        // AI Backend Detection (not in fast lookup)
        "ai_backends" | "ai-backends" => bi_ai_backends(args, input),
        "ai_detect" | "ai-detect" => bi_ai_detect(args, input),

        _ => Err(anyhow!("unknown builtin: {}", name)),
    }
}

// --------------- Helpers: type extraction ---------------

fn expect_array<'a>(name: &str, v: &'a Value) -> Result<&'a [Value]> {
    if let Value::Array(a) = v {
        Ok(a.as_slice())
    } else {
        Err(anyhow!("{} requires array input, got {:?}", name, v))
    }
}

fn expect_int(name: &str, v: &Value) -> Result<i64> {
    match v {
        Value::Int(n) => Ok(*n),
        Value::Float(f) if f.fract() == 0.0 => Ok(*f as i64),
        _ => Err(anyhow!("{} expects integer, got {:?}", name, v)),
    }
}

fn expect_string<'a>(name: &str, v: &'a Value) -> Result<&'a str> {
    if let Value::Str(s) = v {
        Ok(s.as_str())
    } else {
        Err(anyhow!("{} requires string input, got {:?}", name, v))
    }
}

fn need_lambda<'a>(v: &'a Value, name: &str) -> Result<&'a Lambda> {
    if let Value::Lambda(l) = v {
        Ok(l)
    } else {
        Err(anyhow!("{} expects lambda, got {:?}", name, v))
    }
}

/// Evaluate a lambda with N positional arguments by temporarily binding its `params`
/// in the environment, then `eval_expr` on its body. Restores env afterwards.
fn call_lambda(lam: &Lambda, args: &[Value], env: &mut Env) -> Result<Value> {
    let params = &lam.params;
    if params.len() != args.len() {
        return Err(anyhow!(
            "lambda expected {} args, got {}",
            params.len(),
            args.len()
        ));
    }

    // Save and clear pipe input to prevent it from leaking into lambda evaluation
    let saved_pipe = env.input().cloned();
    env.set_input(None);

    // Save previous bindings
    let mut saved: Vec<(String, Option<Value>)> = Vec::with_capacity(params.len());
    for (i, p) in params.iter().enumerate() {
        let prev = env.get_var(p).cloned();
        saved.push((p.clone(), prev));
        env.set_var_unchecked(p, args[i].clone());
    }

    // Eval and restore
    let result = eval_expr(&lam.body, env);

    for (name, prev) in saved.into_iter().rev() {
        match prev {
            Some(v) => env.set_var_unchecked(&name, v),
            None => env.del_var(&name),
        }
    }

    // Restore pipe input
    match saved_pipe {
        Some(v) => env.set_input(Some(v)),
        None => env.set_input(None),
    }

    result
}

// --------------- General builtins ---------------

fn bi_help() -> Result<Value> {
    let txt = r#"Æther (æ) built-ins:
- help                         # this help
- clear                        # clear screen (prints ANSI)
- echo <...values>             # echo stringified values
- print <value>                # pretty-print value (returns text)

Data / pipelines:
- map    <array> <fn(x)=> expr>             # map over array
- where  <array> <fn(x)=> predicate>        # filter array
- reduce <array> <fn(x,y)=> expr> <init>    # fold array
- take   <array> <n>                        # take first n elements
- first  <array>                            # get first element
- last   <array>                            # get last element
- any    <array> [fn(x)=> pred]             # check if any match
- all    <array> [fn(x)=> pred]             # check if all match
- keys   <record>                           # get record keys
- len    <array|record|string>              # get length
- type_of <value>                           # get type name

String functions:
- split      <string> <delimiter>           # split into array
- join       <array> <delimiter>            # join into string
- trim       <string>                       # remove whitespace
- upper      <string>                       # to uppercase
- lower      <string>                       # to lowercase
- replace    <string> <old> <new>           # replace substring
- contains   <string> <substring>           # check if contains
- starts_with <string> <prefix>             # check prefix
- ends_with  <string> <suffix>              # check suffix

Array functions:
- flatten    <array>                        # flatten nested arrays
- reverse    <array|string>                 # reverse order
- slice      <array|string> <start> [end]   # extract slice
- range      <end>  OR  <start> <end> [step] # generate range
- zip        <array1> <array2>              # zip into pairs
- push       <array> <item>                 # append item
- concat     <array1> <array2>              # concatenate
- sort       <array>                        # sort array
- uniq       <array>                        # unique values

Math functions:
- abs        <number>                       # absolute value
- min        <a> <b>                        # minimum
- max        <a> <b>                        # maximum
- floor      <number>                       # round down
- ceil       <number>                       # round up
- round      <number>                       # round nearest
- sqrt       <number>                       # square root
- pow        <base> <exponent>              # power

Utility functions:
- exit       [code]                         # exit program
- env        <key>                          # get env variable
- set_env    <key> <value>                  # set env variable
- sleep      <seconds>                      # sleep duration
- time       ()                             # current timestamp
- json_parse <json_string>                  # parse JSON
- json_stringify <value>                    # to JSON string

File system:
- ls         [path]                         # list directory
- pwd        ()                             # current directory
- cat        <file>                         # read file
- read_text  <file>                         # read file (alias)
- head       <file> [n]                     # first n lines
- tail       <file> [n]                     # last n lines
- find       <path> [pattern]               # find files
- grep       <file|array> <pattern>         # search lines
- wc         <file|array>                   # count lines

HTTP:
- http_get <url>                # fetch URL → {url,status,headers,body}

AI Backend Detection:
- ai_backends                   # list available AI backends with details
- ai_detect                     # auto-select best available AI backend

MCP Server Integration (Discovery):
- mcp_servers                   # list available MCP servers and tools
- mcp_detect [endpoint]         # find specific MCP server or first available
- mcp_cache_clear               # clear MCP detection cache
- mcp_cache_status              # get cache status info

MCP Server/Client (Enhanced):
- mcp_server [config]           # create local MCP server for AI tools
- mcp_tools  [filter]           # list MCP tools with schemas
- mcp_call   <name> [args]      # call MCP tool by name
- mcp_resources [type]          # list MCP resources
- mcp_prompts [name]            # list prompts or get specific one
- mcp_connect <endpoint>        # connect to external MCP server

OS Tools (40+ tools for AI agents):
- tools       [category]        # list available OS tools
- tool_info   <name>            # get detailed tool information
- tool_search <query>           # search tools by keyword
- tool_schema <name>            # get tool input schema
- tool_exec   <name> [args]     # execute tool

Syntax Knowledge Base:
- syntax_get      <id>                      # get syntax entry by ID
- syntax_list     [category]                # list all syntax IDs (optional filter)
- syntax_search   <query>                   # search entries by keyword
- syntax_add      <record>                  # add new syntax entry
- syntax_categories ()                      # list all categories
- ab_encode       <msg_type> <opcode> <payload>  # encode AgenticBinary message
- ab_decode       <bytes>                   # decode AgenticBinary message

AI / Agents (require ai module present):
- agent <goal> [tools...] [max_steps] [dry_run]
- swarm <json-config|record>  OR  <goal> [tools...] [max_steps] [dry_run]

Recursive Language Models (RLM):
- rlm_agent <goal> [tools] [config]         # run agent that can spawn subagents
- rlm_config {opts}                         # create RLM configuration
- rlm_stats  ()                             # get hierarchy statistics
- rlm_spawn  <name> <goal> [tools]          # spawn a single subagent

RLM Configuration Options:
  {max_depth: 5, max_agents: 50, timeout: 60, trace: true}

Examples:
  # Pipelines
  [1,2,3] | map(fn(x)=> x*2) | reduce(fn(a,b)=> a+b, 0)
  [5,4,3,2,1] | where(fn(x)=> x>2) | take(2) | print
  
  # String operations
  "hello,world" | split(",") | map(fn(s)=> s | upper()) | join("-")
  
  # Array operations
  range(10) | where(fn(x)=> x % 2 == 0) | print
  [[1,2],[3,4]] | flatten() | reverse()
  
  # Math
  range(1, 11) | map(fn(x)=> pow(x, 2)) | reduce(fn(a,b)=> a+b, 0)
  
  # JSON
  {name: "Alice", age: 30} | json_stringify() | json_parse()
  
AI & MCP Integration:
  model = ai_detect()            # auto-detect AI backend
  server = mcp_detect()          # find MCP server
  agent("task", model, server.tools)  # use both together
"#;
    Ok(Value::Str(txt.to_string()))
}

fn bi_clear() -> Result<Value> {
    // CSI 2J (clear screen) + CSI H (cursor home)
    Ok(Value::Str("\u{1b}[2J\u{1b}[H".to_string()))
}

fn bi_echo(args: &[Value]) -> Result<Value> {
    let mut s = String::new();
    for (i, v) in args.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format_value_one_line(v));
    }
    Ok(Value::Str(s))
}

fn bi_print(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    // Determine the target Value (either piped input or first arg).
    let target = if let Some(v) = input {
        v
    } else {
        args.into_iter()
            .next()
            .ok_or_else(|| anyhow!("print expects a value"))?
    };

    // (Removed an unused strip_ansi helper; tests have their own)

    // Print the pretty inline display to stdout (with colors).
    let pretty =
        crate::value::pretty::display_inline(&target, &crate::value::pretty::Theme::default());
    println!("{}", pretty);

    // Helper to strip ANSI color sequences from a string
    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut it = s.chars().peekable();
        while let Some(ch) = it.next() {
            if ch == '\x1b' {
                if let Some('[') = it.peek() {
                    it.next();
                    while let Some(&c) = it.peek() {
                        it.next();
                        if c == 'm' {
                            break;
                        }
                    }
                    continue;
                }
            }
            out.push(ch);
        }
        out
    }

    // Return behavior: always return the pretty inline display (ANSI stripped)
    // as a `Str`. This keeps the REPL and tests consistent regardless of
    // whether the input was a bare string or another value.
    Ok(Value::Str(strip_ansi(&pretty)))
}

fn bi_http_get(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let url_val = if let Some(v) = input {
        v
    } else {
        args.into_iter()
            .next()
            .ok_or_else(|| anyhow!("http_get expects URL string/uri"))?
    };
    let url = match url_val {
        Value::Str(s) | Value::Uri(s) => s,
        other => return Err(anyhow!("http_get expects URL string/uri, got {:?}", other)),
    };

    // SECURITY FIX (MED-008): Validate URL to prevent SSRF
    let validated_url = validate_http_url(&url).context("http_get: URL validation failed")?;

    // SECURITY FIX (HIGH-005): Use secure HTTP client
    let client = create_secure_http_client().context("Failed to create HTTP client")?;
    let resp = client.get(&validated_url).send()?;
    let status = resp.status().as_u16() as i64;
    let mut headers_rec = BTreeMap::<String, Value>::new();
    for (k, v) in resp.headers().iter() {
        headers_rec.insert(
            k.to_string(),
            Value::Str(v.to_str().unwrap_or("").to_string()),
        );
    }
    let body_text = resp.text().unwrap_or_default();

    let mut out = BTreeMap::<String, Value>::new();
    out.insert("url".into(), Value::Str(validated_url));
    out.insert("status".into(), Value::Int(status));
    out.insert("headers".into(), Value::Record(headers_rec));
    out.insert("body".into(), Value::Str(body_text));
    Ok(Value::Record(out))
}

fn bi_call(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // call <name> [args...]
    let name_val = args
        .get(0)
        .cloned()
        .ok_or_else(|| anyhow!("call requires name"))?;
    let name = match name_val {
        Value::Str(s) | Value::Uri(s) => s,
        other => {
            return Err(anyhow!(
                "call: expected String builtin name, got {:?}",
                other
            ));
        }
    };
    // remaining args (after name)
    let mut rem: Vec<Value> = Vec::new();
    for v in args.into_iter().skip(1) {
        rem.push(v);
    }
    // If input was provided, pass it as pipe input to the called builtin
    call_with_input(&name, rem, input, env)
}

// --------------- Data / pipeline builtins ---------------

fn bi_map(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // Array comes from pipe if present; else first arg. Use references to avoid moving `input`.
    let arr_val = if let Some(ref v) = input {
        v.clone()
    } else {
        args.get(0)
            .cloned()
            .ok_or_else(|| anyhow!("map requires array input"))?
    };

    let lam_val = if input.is_some() {
        args.get(0).ok_or_else(|| anyhow!("map requires lambda"))?
    } else {
        args.get(1).ok_or_else(|| anyhow!("map requires lambda"))?
    };
    let lam = need_lambda(lam_val, "map")?;

    let arr = expect_array("map", &arr_val)?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, v) in arr.iter().cloned().enumerate() {
        // clone v/acc before trying fallbacks to avoid use-after-move
        let v_clone = v.clone();
        let y = call_lambda(lam, &[v.clone(), Value::Int(i as i64)], env)
            .or_else(|_| call_lambda(lam, &[v_clone], env))?; // support fn(x,i) or fn(x)
        out.push(y);
    }
    Ok(Value::Array(out))
}

fn bi_where(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let arr_val = if let Some(ref v) = input {
        v.clone()
    } else {
        args.get(0)
            .cloned()
            .ok_or_else(|| anyhow!("where requires array input"))?
    };

    let lam_val = if input.is_some() {
        args.get(0)
            .ok_or_else(|| anyhow!("where requires lambda"))?
    } else {
        args.get(1)
            .ok_or_else(|| anyhow!("where requires lambda"))?
    };
    let lam = need_lambda(lam_val, "where")?;

    let arr = expect_array("where", &arr_val)?;
    let mut out = Vec::new();
    for (i, v) in arr.iter().cloned().enumerate() {
        let v_clone = v.clone();
        let keep_val = call_lambda(lam, &[v_clone.clone(), Value::Int(i as i64)], env)
            .or_else(|_| call_lambda(lam, &[v_clone], env))?;
        match keep_val {
            Value::Bool(true) => out.push(v),
            Value::Bool(false) => {}
            other => return Err(anyhow!("where predicate must return Bool, got {:?}", other)),
        }
    }
    Ok(Value::Array(out))
}

fn bi_reduce(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // Cases:
    //  - piping:  [1,2,3] | reduce fn(a,b)=>... 0
    //  - direct:  reduce [1,2,3] fn(a,b)=>... 0
    // Determine arr, lambda index and init index without moving `input`
    let arr_val = if let Some(ref v) = input {
        v.clone()
    } else {
        if args.len() < 3 {
            return Err(anyhow!("reduce expects <array> <fn(a,b)=> expr> <init>"));
        }
        args[0].clone()
    };
    let (lam_idx, init_idx) = if input.is_some() {
        (0usize, 1usize)
    } else {
        (1usize, 2usize)
    };

    let lam = need_lambda(
        args.get(lam_idx)
            .ok_or_else(|| anyhow!("reduce missing lambda"))?,
        "reduce",
    )?;
    let init = args
        .get(init_idx)
        .cloned()
        .ok_or_else(|| anyhow!("reduce missing init"))?;

    let arr = expect_array("reduce", &arr_val)?;
    let mut acc = init;
    for (i, v) in arr.iter().cloned().enumerate() {
        // try fn(a,b,i) then fallback to fn(a,b); clone acc before trying fallbacks
        let acc1 = acc.clone();
        let acc2 = acc.clone();
        acc = call_lambda(lam, &[acc1, v.clone(), Value::Int(i as i64)], env)
            .or_else(|_| call_lambda(lam, &[acc2, v], env))?;
    }
    Ok(acc)
}

fn bi_take(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr_val, n_idx) = if let Some(input_val) = input {
        // SECURITY: Replace .unwrap() with explicit match (CVSS 7.1)
        (input_val, 0usize)
    } else {
        if args.len() < 2 {
            return Err(anyhow!("take expects <array> <n>"));
        }
        (args[0].clone(), 1usize)
    };

    let n = expect_int(
        "take",
        args.get(n_idx).ok_or_else(|| anyhow!("take missing n"))?,
    )?;
    let arr = expect_array("take", &arr_val)?;

    let take_n = if n < 0 { 0 } else { n as usize };
    Ok(Value::Array(arr.iter().cloned().take(take_n).collect()))
}

fn bi_keys(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if let Some(input_val) = input {
        input_val
    } else if !args.is_empty() {
        args[0].clone()
    } else {
        return Err(anyhow!("keys: requires a Record as input or argument"));
    };

    match val {
        Value::Record(map) => {
            let keys: Vec<Value> = map.keys().map(|k| Value::Str(k.clone())).collect();
            Ok(Value::Array(keys))
        }
        _ => Err(anyhow!("keys: requires a Record, got {:?}", val)),
    }
}

fn bi_values(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if let Some(input_val) = input {
        input_val
    } else if !args.is_empty() {
        args[0].clone()
    } else {
        return Err(anyhow!("values: requires a Record as input or argument"));
    };

    match val {
        Value::Record(map) => {
            let values: Vec<Value> = map.values().cloned().collect();
            Ok(Value::Array(values))
        }
        _ => Err(anyhow!("values: requires a Record, got {:?}", val)),
    }
}

fn bi_sum(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let arr = if let Some(input_val) = input {
        input_val
    } else if !args.is_empty() {
        args[0].clone()
    } else {
        return Err(anyhow!("sum: requires an Array as input or argument"));
    };

    match arr {
        Value::Array(items) => {
            let mut int_sum: i64 = 0;
            let mut float_sum: f64 = 0.0;
            let mut has_float = false;

            for item in items {
                match item {
                    Value::Int(n) => {
                        if has_float {
                            float_sum += n as f64;
                        } else {
                            int_sum += n;
                        }
                    }
                    Value::Float(f) => {
                        if !has_float {
                            float_sum = int_sum as f64;
                            has_float = true;
                        }
                        float_sum += f;
                    }
                    _ => {
                        return Err(anyhow!(
                            "sum: array must contain only numbers, got {:?}",
                            item
                        ))
                    }
                }
            }

            if has_float {
                Ok(Value::Float(float_sum))
            } else {
                Ok(Value::Int(int_sum))
            }
        }
        _ => Err(anyhow!("sum: requires an Array, got {:?}", arr)),
    }
}

fn bi_unique(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let array = if let Some(input) = input {
        match input {
            Value::Array(arr) => arr,
            Value::Str(s) => {
                let lines: Vec<Value> =
                    s.lines().map(|line| Value::Str(line.to_string())).collect();
                lines
            }
            _ => return Err(anyhow!("unique: input must be an array or string")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Array(arr) => arr.clone(),
            _ => return Err(anyhow!("unique: argument must be an array")),
        }
    } else {
        return Err(anyhow!("unique: no input provided"));
    };

    // True unique - removes all duplicates (not just consecutive)
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();

    for item in array {
        let key = format!("{:?}", item);
        if seen.insert(key) {
            unique.push(item);
        }
    }

    Ok(Value::Array(unique))
}

fn bi_avg(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let arr = if let Some(input_val) = input {
        input_val
    } else if !args.is_empty() {
        args[0].clone()
    } else {
        return Err(anyhow!("avg: requires an Array as input or argument"));
    };

    match arr {
        Value::Array(items) => {
            if items.is_empty() {
                return Ok(Value::Float(f64::NAN));
            }

            let mut sum: f64 = 0.0;
            for item in &items {
                match item {
                    Value::Int(n) => sum += *n as f64,
                    Value::Float(f) => sum += *f,
                    _ => {
                        return Err(anyhow!(
                            "avg: array must contain only numbers, got {:?}",
                            item
                        ))
                    }
                }
            }

            Ok(Value::Float(sum / items.len() as f64))
        }
        _ => Err(anyhow!("avg: requires an Array, got {:?}", arr)),
    }
}

fn bi_product(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let arr = if let Some(input_val) = input {
        input_val
    } else if !args.is_empty() {
        args[0].clone()
    } else {
        return Err(anyhow!("product: requires an Array as input or argument"));
    };

    match arr {
        Value::Array(items) => {
            let mut int_product: i64 = 1;
            let mut float_product: f64 = 1.0;
            let mut has_float = false;

            for item in items {
                match item {
                    Value::Int(n) => {
                        if has_float {
                            float_product *= n as f64;
                        } else {
                            int_product *= n;
                        }
                    }
                    Value::Float(f) => {
                        if !has_float {
                            float_product = int_product as f64;
                            has_float = true;
                        }
                        float_product *= f;
                    }
                    _ => {
                        return Err(anyhow!(
                            "product: array must contain only numbers, got {:?}",
                            item
                        ))
                    }
                }
            }

            if has_float {
                Ok(Value::Float(float_product))
            } else {
                Ok(Value::Int(int_product))
            }
        }
        _ => Err(anyhow!("product: requires an Array, got {:?}", arr)),
    }
}

// --------------- Configuration builtins ---------------

/// Get the entire configuration as a Record
fn bi_config(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let config = get_config();
    let mut map = BTreeMap::new();

    // Shell settings
    let mut shell = BTreeMap::new();
    shell.insert(
        "show_banner".to_string(),
        Value::Bool(config.shell.show_banner),
    );
    shell.insert("show_tips".to_string(), Value::Bool(config.shell.show_tips));
    shell.insert("vi_mode".to_string(), Value::Bool(config.shell.vi_mode));
    shell.insert("auto_cd".to_string(), Value::Bool(config.shell.auto_cd));
    shell.insert(
        "glob_expansion".to_string(),
        Value::Bool(config.shell.glob_expansion),
    );
    shell.insert(
        "command_correction".to_string(),
        Value::Bool(config.shell.command_correction),
    );
    shell.insert(
        "bell_style".to_string(),
        Value::Str(config.shell.bell_style.clone()),
    );
    map.insert("shell".to_string(), Value::Record(shell));

    // Color settings
    let mut colors = BTreeMap::new();
    colors.insert("enabled".to_string(), Value::Bool(config.colors.enabled));
    colors.insert("theme".to_string(), Value::Str(config.colors.theme.clone()));
    colors.insert("force".to_string(), Value::Bool(config.colors.force));
    colors.insert(
        "true_color".to_string(),
        Value::Bool(config.colors.true_color),
    );
    map.insert("colors".to_string(), Value::Record(colors));

    // Prompt settings
    let mut prompt = BTreeMap::new();
    prompt.insert(
        "format".to_string(),
        Value::Str(config.prompt.format.clone()),
    );
    prompt.insert("show_git".to_string(), Value::Bool(config.prompt.show_git));
    prompt.insert(
        "show_time".to_string(),
        Value::Bool(config.prompt.show_time),
    );
    prompt.insert(
        "time_threshold_ms".to_string(),
        Value::Int(config.prompt.time_threshold_ms as i64),
    );
    map.insert("prompt".to_string(), Value::Record(prompt));

    // AI settings
    let mut ai = BTreeMap::new();
    ai.insert(
        "default_provider".to_string(),
        Value::Str(config.ai.default_provider.clone()),
    );
    ai.insert(
        "default_model".to_string(),
        Value::Str(config.ai.default_model.clone()),
    );
    ai.insert(
        "suggestions".to_string(),
        Value::Bool(config.ai.suggestions),
    );
    ai.insert(
        "max_tokens".to_string(),
        Value::Int(config.ai.max_tokens as i64),
    );
    ai.insert(
        "temperature".to_string(),
        Value::Float(config.ai.temperature as f64),
    );
    ai.insert("streaming".to_string(), Value::Bool(config.ai.streaming));
    ai.insert(
        "max_agent_steps".to_string(),
        Value::Int(config.ai.max_agent_steps as i64),
    );
    map.insert("ai".to_string(), Value::Record(ai));

    // History settings
    let mut history = BTreeMap::new();
    history.insert("enabled".to_string(), Value::Bool(config.history.enabled));
    history.insert(
        "max_size".to_string(),
        Value::Int(config.history.max_size as i64),
    );
    history.insert(
        "ignore_duplicates".to_string(),
        Value::Bool(config.history.ignore_duplicates),
    );
    history.insert(
        "ignore_space".to_string(),
        Value::Bool(config.history.ignore_space),
    );
    history.insert("share".to_string(), Value::Bool(config.history.share));
    history.insert(
        "timestamps".to_string(),
        Value::Bool(config.history.timestamps),
    );
    map.insert("history".to_string(), Value::Record(history));

    Ok(Value::Record(map))
}

/// Get a specific config value by path (e.g., "colors.theme" or "ai.default_model")
fn bi_config_get(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!(
            "config_get: requires a path argument (e.g., \"colors.theme\")"
        ));
    }

    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(anyhow!("config_get: path must be a string")),
    };

    let config = get_config();
    let parts: Vec<&str> = path.split('.').collect();

    match parts.as_slice() {
        // Shell settings
        ["shell", "show_banner"] => Ok(Value::Bool(config.shell.show_banner)),
        ["shell", "show_tips"] => Ok(Value::Bool(config.shell.show_tips)),
        ["shell", "vi_mode"] => Ok(Value::Bool(config.shell.vi_mode)),
        ["shell", "auto_cd"] => Ok(Value::Bool(config.shell.auto_cd)),
        ["shell", "glob_expansion"] => Ok(Value::Bool(config.shell.glob_expansion)),
        ["shell", "command_correction"] => Ok(Value::Bool(config.shell.command_correction)),
        ["shell", "bell_style"] => Ok(Value::Str(config.shell.bell_style.clone())),

        // Color settings
        ["colors", "enabled"] => Ok(Value::Bool(config.colors.enabled)),
        ["colors", "theme"] => Ok(Value::Str(config.colors.theme.clone())),
        ["colors", "force"] => Ok(Value::Bool(config.colors.force)),
        ["colors", "true_color"] => Ok(Value::Bool(config.colors.true_color)),

        // Prompt settings
        ["prompt", "format"] => Ok(Value::Str(config.prompt.format.clone())),
        ["prompt", "show_git"] => Ok(Value::Bool(config.prompt.show_git)),
        ["prompt", "show_time"] => Ok(Value::Bool(config.prompt.show_time)),
        ["prompt", "time_threshold_ms"] => Ok(Value::Int(config.prompt.time_threshold_ms as i64)),

        // AI settings
        ["ai", "default_provider"] => Ok(Value::Str(config.ai.default_provider.clone())),
        ["ai", "default_model"] => Ok(Value::Str(config.ai.default_model.clone())),
        ["ai", "suggestions"] => Ok(Value::Bool(config.ai.suggestions)),
        ["ai", "max_tokens"] => Ok(Value::Int(config.ai.max_tokens as i64)),
        ["ai", "temperature"] => Ok(Value::Float(config.ai.temperature as f64)),
        ["ai", "streaming"] => Ok(Value::Bool(config.ai.streaming)),
        ["ai", "max_agent_steps"] => Ok(Value::Int(config.ai.max_agent_steps as i64)),

        // History settings
        ["history", "enabled"] => Ok(Value::Bool(config.history.enabled)),
        ["history", "max_size"] => Ok(Value::Int(config.history.max_size as i64)),
        ["history", "ignore_duplicates"] => Ok(Value::Bool(config.history.ignore_duplicates)),
        ["history", "ignore_space"] => Ok(Value::Bool(config.history.ignore_space)),
        ["history", "share"] => Ok(Value::Bool(config.history.share)),
        ["history", "timestamps"] => Ok(Value::Bool(config.history.timestamps)),

        _ => Err(anyhow!("config_get: unknown config path: {}", path)),
    }
}

/// Set a config value (saves to config file)
/// Note: This modifies the config file but changes only take effect after reload
fn bi_config_set(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("config_set: requires path and value arguments"));
    }

    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(anyhow!("config_set: path must be a string")),
    };

    // Load current config, modify it, and save
    let mut config = ShellConfig::load()?;
    let parts: Vec<&str> = path.split('.').collect();

    match (parts.as_slice(), &args[1]) {
        // Color settings (most commonly changed)
        (["colors", "enabled"], Value::Bool(v)) => config.colors.enabled = *v,
        (["colors", "theme"], Value::Str(v)) => config.colors.theme = v.clone(),
        (["colors", "force"], Value::Bool(v)) => config.colors.force = *v,
        (["colors", "true_color"], Value::Bool(v)) => config.colors.true_color = *v,

        // Shell settings
        (["shell", "show_banner"], Value::Bool(v)) => config.shell.show_banner = *v,
        (["shell", "show_tips"], Value::Bool(v)) => config.shell.show_tips = *v,
        (["shell", "vi_mode"], Value::Bool(v)) => config.shell.vi_mode = *v,
        (["shell", "auto_cd"], Value::Bool(v)) => config.shell.auto_cd = *v,

        // AI settings
        (["ai", "default_model"], Value::Str(v)) => config.ai.default_model = v.clone(),
        (["ai", "default_provider"], Value::Str(v)) => config.ai.default_provider = v.clone(),
        (["ai", "max_tokens"], Value::Int(v)) => config.ai.max_tokens = *v as u32,
        (["ai", "streaming"], Value::Bool(v)) => config.ai.streaming = *v,

        // History settings
        (["history", "enabled"], Value::Bool(v)) => config.history.enabled = *v,
        (["history", "max_size"], Value::Int(v)) => config.history.max_size = *v as usize,

        _ => {
            return Err(anyhow!(
                "config_set: cannot set path '{}' or invalid value type",
                path
            ))
        }
    }

    config.save()?;
    Ok(Value::Str(format!(
        "Config saved: {} = {:?}",
        path, args[1]
    )))
}

/// Get config file paths
fn bi_config_path() -> Result<Value> {
    let mut map = BTreeMap::new();
    map.insert(
        "config_file".to_string(),
        Value::Str(ShellConfig::config_file().to_string_lossy().to_string()),
    );
    map.insert(
        "config_dir".to_string(),
        Value::Str(ShellConfig::config_dir().to_string_lossy().to_string()),
    );
    map.insert(
        "data_dir".to_string(),
        Value::Str(ShellConfig::data_dir().to_string_lossy().to_string()),
    );
    map.insert(
        "cache_dir".to_string(),
        Value::Str(ShellConfig::cache_dir().to_string_lossy().to_string()),
    );
    map.insert(
        "history_file".to_string(),
        Value::Str(ShellConfig::history_file().to_string_lossy().to_string()),
    );
    map.insert(
        "plugins_dir".to_string(),
        Value::Str(ShellConfig::plugins_dir().to_string_lossy().to_string()),
    );
    map.insert(
        "init_script".to_string(),
        Value::Str(ShellConfig::init_script().to_string_lossy().to_string()),
    );
    Ok(Value::Record(map))
}

/// Initialize config directories and create default config file
fn bi_config_init() -> Result<Value> {
    ShellConfig::init_dirs()?;

    let config_file = ShellConfig::config_dir().join("config.toml");
    if !config_file.exists() {
        let default_config = ShellConfig::generate_default_config();
        fs::write(&config_file, default_config)?;
        Ok(Value::Str(format!(
            "Created config file: {}",
            config_file.display()
        )))
    } else {
        Ok(Value::Str(format!(
            "Config file already exists: {}",
            config_file.display()
        )))
    }
}

/// Reload configuration from disk
fn bi_config_reload() -> Result<Value> {
    match reload_config() {
        Ok(_) => Ok(Value::Str("Configuration reloaded".to_string())),
        Err(e) => Err(anyhow!("Failed to reload config: {}", e)),
    }
}

/// List all available color themes
fn bi_themes() -> Result<Value> {
    use crate::config::Theme;
    let themes: Vec<Value> = Theme::list()
        .iter()
        .map(|s| Value::Str(s.to_string()))
        .collect();
    Ok(Value::Array(themes))
}

// ===================== Plugin Builtins =====================

/// List all registered plugins
fn bi_plugins(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    Ok(crate::plugins::bi_plugins_list())
}

/// Get detailed information about a plugin
fn bi_plugin_info(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plugin_id = if let Some(Value::Str(id)) = input {
        id
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(id) => id.clone(),
            _ => return Err(anyhow!("plugin_info: requires plugin ID string")),
        }
    } else {
        return Err(anyhow!("plugin_info: requires plugin ID"));
    };

    Ok(crate::plugins::bi_plugin_info(&plugin_id))
}

/// Enable a plugin by ID
fn bi_plugin_enable(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plugin_id = if let Some(Value::Str(id)) = input {
        id
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(id) => id.clone(),
            _ => return Err(anyhow!("plugin_enable: requires plugin ID string")),
        }
    } else {
        return Err(anyhow!("plugin_enable: requires plugin ID"));
    };

    crate::plugins::bi_plugin_enable(&plugin_id)
}

/// Disable a plugin by ID
fn bi_plugin_disable(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plugin_id = if let Some(Value::Str(id)) = input {
        id
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(id) => id.clone(),
            _ => return Err(anyhow!("plugin_disable: requires plugin ID string")),
        }
    } else {
        return Err(anyhow!("plugin_disable: requires plugin ID"));
    };

    crate::plugins::bi_plugin_disable(&plugin_id)
}

/// Load a plugin from a manifest file path
fn bi_plugin_load(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let path = if let Some(Value::Str(p)) = input {
        p
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(p) => p.clone(),
            _ => return Err(anyhow!("plugin_load: requires path string")),
        }
    } else {
        return Err(anyhow!("plugin_load: requires manifest path"));
    };

    crate::plugins::load_plugin_from_manifest(&path)
}

/// Unload a plugin by ID
fn bi_plugin_unload(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plugin_id = if let Some(Value::Str(id)) = input {
        id
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(id) => id.clone(),
            _ => return Err(anyhow!("plugin_unload: requires plugin ID string")),
        }
    } else {
        return Err(anyhow!("plugin_unload: requires plugin ID"));
    };

    crate::plugins::unload_plugin(&plugin_id)
}

/// List all plugin categories
fn bi_plugin_categories() -> Result<Value> {
    Ok(crate::plugins::bi_plugin_categories())
}

fn bi_len(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if let Some(input_val) = input {
        input_val
    } else if !args.is_empty() {
        args[0].clone()
    } else {
        return Err(anyhow!("len: requires input or argument"));
    };

    let length = match val {
        Value::Array(ref v) => v.len(),
        Value::Record(ref m) => m.len(),
        Value::Str(ref s) => s.len(),
        _ => {
            return Err(anyhow!(
                "len: requires Array, Record, or String, got {:?}",
                val
            ));
        }
    };

    Ok(Value::Int(length as i64))
}

fn bi_type_of(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if let Some(input_val) = input {
        input_val
    } else if !args.is_empty() {
        args[0].clone()
    } else {
        return Err(anyhow!("type_of: requires input or argument"));
    };

    let type_str = match val {
        Value::Null => "Null",
        Value::Bool(_) => "Bool",
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::Str(_) => "String",
        Value::Uri(_) => "Uri",
        Value::Array(_) => "Array",
        Value::Record(_) => "Record",
        Value::Table(_) => "Table",
        Value::Lambda(_) => "Lambda",
        Value::AsyncLambda(_) => "AsyncLambda",
        Value::Future(_) => "Future",
    };

    Ok(Value::Str(type_str.to_string()))
}

// --------------- AI / Agents wrappers (optional) ---------------

fn bi_agent(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // SECURITY: Rate limit agent calls (CVSS 7.8)
    check_rate_limit("bi_agent", 10, Duration::from_secs(60))
        .context("Agent rate limit exceeded")?;

    // Accept either: agent "<goal>" [tools...] [max_steps] [dry_run]
    // or a record config: {goal:"...", tools:["..."], max_steps:3, dry_run:true, model_uri:"..."}
    if let Some(Value::Record(cfg)) = input {
        return agent_from_record(cfg, env);
    }
    if let Some(Value::Record(cfg)) = args.get(0) {
        return agent_from_record(cfg.clone(), env);
    }
    // positional
    let goal_str = match args.get(0) {
        Some(Value::Str(s)) => s.clone(),
        Some(other) => return Err(anyhow!("agent: expected String goal, got {:?}", other)),
        None => return Err(anyhow!("agent requires goal")),
    };

    // SECURITY: Validate AI prompt for injection (CVSS 7.8)
    let goal = validate_ai_prompt(&goal_str).context("agent: goal validation failed")?;
    let mut tools: Vec<String> = Vec::new();
    let mut max_steps: usize = 8;
    let mut dry_run = false;

    // tools: collect following strings until number/bool appears
    let mut i = 1usize;
    // If second positional arg is a JSON config string, try to parse it
    if let Some(Value::Str(s)) = args.get(1) {
        let t = s.trim();
        if t.starts_with('{') {
            // try parse JSON into a Record Value and delegate, merge goal
            if let Ok(jv) = serde_json::from_str::<serde_json::Value>(t) {
                let mut cfg_val = crate::value::Value::from_json(&jv);
                if let Value::Record(ref mut cfg) = cfg_val {
                    // if we have a goal string in args[0], ensure the record has goal
                    if let Some(Value::Str(goal_s)) = args.get(0) {
                        cfg.insert("goal".into(), Value::Str(goal_s.clone()));
                        return agent_from_record(cfg.clone(), env);
                    } else {
                        return Err(anyhow!(
                            "agent config must include goal when provided as JSON"
                        ));
                    }
                } else {
                    return Err(anyhow!("agent config must be a record"));
                }
            } else {
                return Err(anyhow!("agent config JSON parse error"));
            }
        }
    }

    while let Some(Value::Str(s)) = args.get(i) {
        tools.push(s.clone());
        i += 1;
    }
    // Also accept a single positional array as tools: agent(goal, ["ls","print"], ...)
    if tools.is_empty() {
        if let Some(Value::Array(vs)) = args.get(1) {
            for v in vs {
                if let Value::Str(s) = v {
                    tools.push(s.clone());
                } else {
                    return Err(anyhow!("agent: tools array must contain strings"));
                }
            }
            i = 2;
        }
    }
    if let Some(v) = args.get(i) {
        if let Value::Int(n) = v {
            max_steps = (*n).max(0) as usize;
            i += 1;
        }
    }
    if let Some(v) = args.get(i) {
        if let Value::Bool(b) = v {
            dry_run = *b;
        }
    }
    let tool_refs: Vec<&str> = tools.iter().map(|s| s.as_str()).collect();
    let out = crate::ai::agents::run_sync(&goal, &tool_refs, max_steps, dry_run, env)?;
    Ok(Value::Str(out))
}

fn agent_from_record(cfg: BTreeMap<String, Value>, env: &mut Env) -> Result<Value> {
    let goal = match cfg.get("goal") {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(anyhow!("agent config requires {{goal: String}}")),
    };
    let tools: Vec<String> = match cfg.get("tools") {
        Some(Value::Array(vs)) => {
            // Validate all tools are strings
            let mut out = Vec::new();
            for v in vs {
                if let Value::Str(s) = v {
                    out.push(s.clone());
                } else {
                    return Err(anyhow!("agent config tools must be array of strings"));
                }
            }
            out
        }
        _ => vec![],
    };
    let max_steps = match cfg.get("max_steps") {
        Some(Value::Int(n)) if *n >= 0 => *n as usize,
        _ => 8usize,
    };
    let dry_run = matches!(cfg.get("dry_run"), Some(Value::Bool(true)));

    let tool_refs: Vec<&str> = tools.iter().map(|s| s.as_str()).collect();
    let out = crate::ai::agents::run_sync(&goal, &tool_refs, max_steps, dry_run, env)?;
    Ok(Value::Str(out))
}

fn bi_swarm(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // Accept {goal, tools, max_steps, dry_run} or just a goal + tools...
    if let Some(Value::Record(cfg)) = input {
        return swarm_from_record(cfg, env);
    }
    if let Some(Value::Record(cfg)) = args.get(0) {
        return swarm_from_record(cfg.clone(), env);
    }

    // positional like agent()
    let goal = match args.get(0) {
        Some(Value::Str(s)) => s.clone(),
        Some(other) => return Err(anyhow!("swarm: expected String goal, got {:?}", other)),
        None => return Err(anyhow!("swarm requires goal")),
    };
    let mut tools: Vec<String> = Vec::new();
    let mut max_steps: usize = 12;
    let mut dry_run = false;

    let mut i = 1usize;
    // If second positional arg is a JSON config string, try parse and delegate
    if let Some(Value::Str(s)) = args.get(1) {
        let t = s.trim();
        if t.starts_with('{') {
            if let Ok(jv) = serde_json::from_str::<serde_json::Value>(t) {
                let mut cfg_val = crate::value::Value::from_json(&jv);
                if let Value::Record(ref mut cfg) = cfg_val {
                    if let Some(Value::Str(goal_s)) = args.get(0) {
                        cfg.insert("goal".into(), Value::Str(goal_s.clone()));
                        return swarm_from_record(cfg.clone(), env);
                    } else {
                        return Err(anyhow!(
                            "swarm config must include goal when provided as JSON"
                        ));
                    }
                } else {
                    return Err(anyhow!("swarm config must be a record"));
                }
            } else {
                return Err(anyhow!("swarm config JSON parse error"));
            }
        }
    }
    while let Some(Value::Str(s)) = args.get(i) {
        tools.push(s.clone());
        i += 1;
    }
    // Also accept tools as a single array positional argument
    if tools.is_empty() {
        if let Some(Value::Array(vs)) = args.get(1) {
            for v in vs {
                if let Value::Str(s) = v {
                    tools.push(s.clone());
                } else {
                    return Err(anyhow!("swarm: tools array must contain strings"));
                }
            }
            i = 2;
        }
    }
    if let Some(v) = args.get(i) {
        if let Value::Int(n) = v {
            max_steps = (*n).max(0) as usize;
            i += 1;
        }
    }
    if let Some(v) = args.get(i) {
        if let Value::Bool(b) = v {
            dry_run = *b;
        }
    }

    let tool_refs: Vec<&str> = tools.iter().map(|s| s.as_str()).collect();
    // For compatibility we call agents::run_sync (single agent) from the swarm shim in ai.rs
    let out = crate::ai::agents::swarm::run_sync(&goal, &tool_refs, max_steps, dry_run, env)?;
    Ok(Value::Str(out))
}

fn swarm_from_record(cfg: BTreeMap<String, Value>, env: &mut Env) -> Result<Value> {
    let goal = match cfg.get("goal") {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(anyhow!("swarm config requires {{goal: String}}")),
    };
    let tools: Vec<String> = match cfg.get("tools") {
        Some(Value::Array(vs)) => {
            let mut out = Vec::new();
            for v in vs {
                if let Value::Str(s) = v {
                    out.push(s.clone());
                } else {
                    return Err(anyhow!("swarm config tools must be array of strings"));
                }
            }
            out
        }
        _ => vec![],
    };
    let max_steps = match cfg.get("max_steps") {
        Some(Value::Int(n)) if *n >= 0 => *n as usize,
        _ => 12usize,
    };
    let dry_run = matches!(cfg.get("dry_run"), Some(Value::Bool(true)));

    let tool_refs: Vec<&str> = tools.iter().map(|s| s.as_str()).collect();
    let out = crate::ai::agents::swarm::run_sync(&goal, &tool_refs, max_steps, dry_run, env)?;
    Ok(Value::Str(out))
}

// --------------- File system commands ---------------

fn bi_ls(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let path_str = if args.is_empty() {
        ".".to_string()
    } else {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("ls: path must be a string")),
        }
    };

    // SECURITY: Validate path to prevent traversal attacks (CVSS 8.2)
    let validated_path = validate_read_path(&path_str).context("ls: path validation failed")?;

    let entries = fs::read_dir(&validated_path)
        .with_context(|| format!("ls: failed to read directory: {:?}", validated_path))?;
    let mut files = Vec::new();

    for entry in entries {
        let entry = entry.with_context(|| "ls: failed to read directory entry")?;
        let metadata = entry
            .metadata()
            .with_context(|| "ls: failed to read file metadata")?;
        let path = entry.path();
        let name = path
            .file_name()
            .ok_or_else(|| anyhow!("ls: invalid filename"))?
            .to_string_lossy()
            .to_string();

        // Extract file extension
        let ext = path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();

        let mut record = BTreeMap::new();
        record.insert("name".to_string(), Value::Str(name));
        record.insert(
            "path".to_string(),
            Value::Str(path.to_string_lossy().to_string()),
        );
        record.insert("ext".to_string(), Value::Str(ext));
        record.insert("is_dir".to_string(), Value::Bool(metadata.is_dir()));
        record.insert("size".to_string(), Value::Int(metadata.len() as i64));

        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                record.insert(
                    "modified".to_string(),
                    Value::Int(duration.as_secs() as i64),
                );
            }
        }

        files.push(Value::Record(record));
    }

    Ok(Value::Array(files))
}

fn bi_pwd() -> Result<Value> {
    let current_dir = std::env::current_dir()?;
    Ok(Value::Str(current_dir.to_string_lossy().to_string()))
}

fn bi_cat(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if let Some(input) = input {
        return Ok(input);
    }

    if args.is_empty() {
        return Err(anyhow!("cat: no file specified"));
    }

    let path_str = match &args[0] {
        Value::Str(s) => s,
        _ => return Err(anyhow!("cat: path must be a string")),
    };

    // SECURITY: Validate path to prevent traversal attacks (CVSS 8.2)
    let validated_path = validate_read_path(path_str).context("cat: path validation failed")?;

    // SECURITY FIX (MED-001): Check file size before reading
    let metadata = fs::metadata(&validated_path)
        .with_context(|| format!("cat: failed to read file metadata: {:?}", validated_path))?;
    check_file_size_limit(metadata.len()).context("cat: file too large")?;

    let content = fs::read_to_string(&validated_path)
        .with_context(|| format!("cat: failed to read file: {:?}", validated_path))?;
    Ok(Value::Str(content))
}

fn bi_read_text(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("read_text: no file specified"));
    }

    let path_str = match &args[0] {
        Value::Str(s) => s,
        _ => return Err(anyhow!("read_text: path must be a string")),
    };

    // SECURITY: Validate path to prevent traversal attacks
    let validated_path =
        validate_read_path(path_str).context("read_text: path validation failed")?;

    // SECURITY FIX (MED-001): Check file size before reading
    let metadata = fs::metadata(&validated_path).with_context(|| {
        format!(
            "read_text: failed to read file metadata: {:?}",
            validated_path
        )
    })?;
    check_file_size_limit(metadata.len()).context("read_text: file too large")?;

    let content = fs::read_to_string(&validated_path)
        .with_context(|| format!("read_text: failed to read file: {:?}", validated_path))?;

    Ok(Value::Str(content))
}

fn bi_head(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    // Determine line count and content source
    let (lines_count, content) = if let Some(input) = input {
        // Input from pipeline
        let count = if !args.is_empty() {
            match &args[0] {
                Value::Int(n) => *n as usize,
                _ => 10,
            }
        } else {
            10
        };
        let content = match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("head: input must be a string")),
        };
        (count, content)
    } else {
        // No pipeline input, read from file
        match args.len() {
            0 => return Err(anyhow!("head: no input provided")),
            1 => {
                // Single argument: could be file path or line count
                match &args[0] {
                    Value::Str(path_str) => {
                        // SECURITY: Validate path to prevent traversal attacks (CVSS 8.2)
                        let validated_path =
                            validate_read_path(path_str).context("head: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("head: failed to read file: {:?}", validated_path)
                        })?;
                        (10, content)
                    }
                    Value::Int(_) => return Err(anyhow!("head: no file specified")),
                    _ => return Err(anyhow!("head: invalid argument")),
                }
            }
            2 => {
                // Two arguments: could be (file, count) or (count, file)
                // Try file first, then count
                match (&args[0], &args[1]) {
                    (Value::Str(path_str), Value::Int(n)) => {
                        // SECURITY: Validate path to prevent traversal attacks
                        let validated_path =
                            validate_read_path(path_str).context("head: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("head: failed to read file: {:?}", validated_path)
                        })?;
                        (*n as usize, content)
                    }
                    (Value::Int(n), Value::Str(path_str)) => {
                        // SECURITY: Validate path to prevent traversal attacks
                        let validated_path =
                            validate_read_path(path_str).context("head: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("head: failed to read file: {:?}", validated_path)
                        })?;
                        (*n as usize, content)
                    }
                    _ => {
                        return Err(anyhow!(
                            "head: invalid arguments - need file path and line count"
                        ));
                    }
                }
            }
            _ => return Err(anyhow!("head: too many arguments")),
        }
    };

    let lines: Vec<&str> = content.lines().take(lines_count).collect();
    Ok(Value::Str(lines.join("\n")))
}

fn bi_tail(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    // Determine line count and content source
    let (lines_count, content) = if let Some(input) = input {
        // Input from pipeline
        let count = if !args.is_empty() {
            match &args[0] {
                Value::Int(n) => *n as usize,
                _ => 10,
            }
        } else {
            10
        };
        let content = match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("tail: input must be a string")),
        };
        (count, content)
    } else {
        // No pipeline input, read from file
        match args.len() {
            0 => return Err(anyhow!("tail: no input provided")),
            1 => {
                // Single argument: could be file path or line count
                match &args[0] {
                    Value::Str(path_str) => {
                        // SECURITY: Validate path to prevent traversal attacks (CVSS 8.2)
                        let validated_path =
                            validate_read_path(path_str).context("tail: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("tail: failed to read file: {:?}", validated_path)
                        })?;
                        (10, content)
                    }
                    Value::Int(_) => return Err(anyhow!("tail: no file specified")),
                    _ => return Err(anyhow!("tail: invalid argument")),
                }
            }
            2 => {
                // Two arguments: could be (file, count) or (count, file)
                match (&args[0], &args[1]) {
                    (Value::Str(path_str), Value::Int(n)) => {
                        // SECURITY: Validate path to prevent traversal attacks
                        let validated_path =
                            validate_read_path(path_str).context("tail: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("tail: failed to read file: {:?}", validated_path)
                        })?;
                        (*n as usize, content)
                    }
                    (Value::Int(n), Value::Str(path_str)) => {
                        // SECURITY: Validate path to prevent traversal attacks
                        let validated_path =
                            validate_read_path(path_str).context("tail: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("tail: failed to read file: {:?}", validated_path)
                        })?;
                        (*n as usize, content)
                    }
                    _ => {
                        return Err(anyhow!(
                            "tail: invalid arguments - need file path and line count"
                        ));
                    }
                }
            }
            _ => return Err(anyhow!("tail: too many arguments")),
        }
    };

    let lines: Vec<&str> = content.lines().collect();
    let start = if lines.len() > lines_count {
        lines.len() - lines_count
    } else {
        0
    };
    let tail_lines = &lines[start..];
    Ok(Value::Str(tail_lines.join("\n")))
}

fn bi_find(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let (path_str, pattern) = if args.len() >= 2 {
        match (&args[0], &args[1]) {
            (Value::Str(p), Value::Str(pat)) => (p.clone(), Some(pat.clone())),
            _ => return Err(anyhow!("find: path and pattern must be strings")),
        }
    } else if args.len() == 1 {
        match &args[0] {
            Value::Str(p) => (p.clone(), None),
            _ => return Err(anyhow!("find: path must be a string")),
        }
    } else {
        (".".to_string(), None)
    };

    // SECURITY: Validate path to prevent traversal attacks (CVSS 8.2)
    let validated_path = validate_read_path(&path_str).context("find: path validation failed")?;

    let mut results = Vec::new();

    for entry in WalkDir::new(&validated_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path_str = entry.path().to_string_lossy().to_string();

        if let Some(ref pattern) = pattern {
            // Simple glob matching: support * for any characters
            let filename = entry.file_name().to_string_lossy();
            if glob_match(pattern, &filename) {
                results.push(Value::Str(path_str));
            }
        } else {
            results.push(Value::Str(path_str));
        }
    }

    Ok(Value::Array(results))
}

// Simple glob matcher for basic patterns like *.ext
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.starts_with("*.") {
        let ext = &pattern[2..];
        return text.ends_with(&format!(".{}", ext));
    }

    if pattern.ends_with("*") {
        let prefix = &pattern[..pattern.len() - 1];
        return text.starts_with(prefix);
    }

    // Exact match if no wildcards
    pattern == text
}

fn bi_sort(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let reverse = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-r" || s == "--reverse"));

    let array = if let Some(input) = input {
        match input {
            Value::Array(arr) => arr,
            Value::Str(s) => {
                let lines: Vec<Value> =
                    s.lines().map(|line| Value::Str(line.to_string())).collect();
                lines
            }
            _ => return Err(anyhow!("sort: input must be an array or string")),
        }
    } else {
        return Err(anyhow!("sort: no input provided"));
    };

    let mut sorted = array;
    sorted.sort_by(|a, b| {
        let cmp = match (a, b) {
            (Value::Str(s1), Value::Str(s2)) => s1.cmp(s2),
            (Value::Int(i1), Value::Int(i2)) => i1.cmp(i2),
            (Value::Float(f1), Value::Float(f2)) => {
                f1.partial_cmp(f2).unwrap_or(std::cmp::Ordering::Equal)
            }
            _ => std::cmp::Ordering::Equal,
        };
        if reverse {
            cmp.reverse()
        } else {
            cmp
        }
    });

    Ok(Value::Array(sorted))
}

fn bi_uniq(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let _count = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-c" || s == "--count"));

    let array = if let Some(input) = input {
        match input {
            Value::Array(arr) => arr,
            Value::Str(s) => {
                let lines: Vec<Value> =
                    s.lines().map(|line| Value::Str(line.to_string())).collect();
                lines
            }
            _ => return Err(anyhow!("uniq: input must be an array or string")),
        }
    } else {
        return Err(anyhow!("uniq: no input provided"));
    };

    let mut unique = Vec::new();
    let mut last: Option<&Value> = None;

    for item in &array {
        if last.map_or(true, |l| l != item) {
            unique.push(item.clone());
            last = Some(item);
        }
    }

    Ok(Value::Array(unique))
}

fn bi_wc(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let lines_only = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-l" || s == "--lines"));
    let words_only = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-w" || s == "--words"));
    let chars_only = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-c" || s == "--chars"));

    let content = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("wc: input must be a string")),
        }
    } else if !args.is_empty() {
        let path = match &args[0] {
            Value::Str(s) if !s.starts_with('-') => s,
            _ => return Err(anyhow!("wc: no input provided")),
        };
        fs::read_to_string(path)?
    } else {
        return Err(anyhow!("wc: no input provided"));
    };

    let line_count = content.lines().count();
    let word_count = content.split_whitespace().count();
    let char_count = content.chars().count();

    let mut result = BTreeMap::new();

    if lines_only {
        result.insert("lines".to_string(), Value::Int(line_count as i64));
    } else if words_only {
        result.insert("words".to_string(), Value::Int(word_count as i64));
    } else if chars_only {
        result.insert("chars".to_string(), Value::Int(char_count as i64));
    } else {
        result.insert("lines".to_string(), Value::Int(line_count as i64));
        result.insert("words".to_string(), Value::Int(word_count as i64));
        result.insert("chars".to_string(), Value::Int(char_count as i64));
    }

    Ok(Value::Record(result))
}

fn bi_grep(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("grep: pattern required"));
    }

    let pattern = match &args[0] {
        Value::Str(s) => s,
        _ => return Err(anyhow!("grep: pattern must be a string")),
    };

    let case_insensitive = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-i" || s == "--ignore-case"));
    let invert = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-v" || s == "--invert-match"));

    let content = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            Value::Array(arr) => {
                let lines: Vec<String> = arr
                    .iter()
                    .filter_map(|v| {
                        if let Value::Str(s) = v {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                lines.join("\n")
            }
            _ => return Err(anyhow!("grep: input must be a string or array")),
        }
    } else if args.len() > 1 {
        let path = match &args[1] {
            Value::Str(s) if !s.starts_with('-') => s,
            _ => return Err(anyhow!("grep: no input provided")),
        };
        fs::read_to_string(path)?
    } else {
        return Err(anyhow!("grep: no input provided"));
    };

    let matching_lines: Vec<Value> = content
        .lines()
        .filter(|line| {
            let matches = if case_insensitive {
                line.to_lowercase().contains(&pattern.to_lowercase())
            } else {
                line.contains(pattern)
            };
            if invert {
                !matches
            } else {
                matches
            }
        })
        .map(|line| Value::Str(line.to_string()))
        .collect();

    Ok(Value::Array(matching_lines))
}

// --------------- Formatting helpers ---------------

fn format_value_one_line(v: &Value) -> String {
    // keep this consistent with your pretty module; simplified for echo
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(x) => x.to_string(),
        Value::Str(s) => s.clone(),
        Value::Uri(s) => s.clone(),
        Value::Array(a) => format!("[len={}]", a.len()),
        Value::Record(_) => "{…}".into(),
        Value::Table(t) => format!("<Table rows={}>", t.rows.len()),
        Value::Lambda(_) => "<lambda>".into(),
        Value::AsyncLambda(_) => "<async lambda>".into(),
        Value::Future(_) => "<future>".into(),
    }
}

// Helper function to convert Value to string representation
fn value_to_string(value: &Value) -> String {
    match value {
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Uri(s) => s.clone(),
        Value::Array(a) => format!("[len={}]", a.len()),
        Value::Record(_) => "{…}".into(),
        Value::Table(t) => format!("<Table rows={}>", t.rows.len()),
        Value::Lambda(_) => "<lambda>".into(),
        Value::AsyncLambda(_) => "<async lambda>".into(),
        Value::Future(_) => "<future>".into(),
        Value::Null => "<null>".into(),
    }
}

// =============== PowerShell-style Parameter Binding ===============

/// PowerShell-style parameter binding result
#[derive(Debug)]
pub struct BoundParameters {
    pub named: BTreeMap<String, Value>,
    pub positional: Vec<Value>,
    pub pipeline_input: Option<Value>,
}

/// Bind parameters according to PowerShell conventions
fn bind_parameters(
    parameter_set: &ParameterSet,
    args: Vec<Value>,
    input: Option<Value>,
) -> Result<BoundParameters> {
    let mut bound = BoundParameters {
        named: BTreeMap::new(),
        positional: Vec::new(),
        pipeline_input: input,
    };

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        // Check if this is a named parameter (starts with -)
        if let Value::Str(s) = arg {
            if s.starts_with('-') {
                let param_name = s.trim_start_matches('-');

                // Find matching parameter definition
                if let Some(param) = parameter_set
                    .parameters
                    .iter()
                    .find(|p| p.name == param_name || p.aliases.contains(&param_name.to_string()))
                {
                    match param.parameter_type {
                        ParameterType::Switch => {
                            // Switch parameters don't take values
                            bound.named.insert(param.name.clone(), Value::Bool(true));
                            i += 1;
                        }
                        _ => {
                            // Other parameters take the next argument as value
                            if i + 1 < args.len() {
                                let value =
                                    validate_parameter_type(&args[i + 1], &param.parameter_type)?;
                                bound.named.insert(param.name.clone(), value);
                                i += 2;
                            } else {
                                return Err(anyhow!("Parameter '{}' requires a value", param.name));
                            }
                        }
                    }
                } else {
                    return Err(anyhow!("Unknown parameter: {}", param_name));
                }
            } else {
                // Positional parameter
                bound.positional.push(arg.clone());
                i += 1;
            }
        } else {
            // Positional parameter
            bound.positional.push(arg.clone());
            i += 1;
        }
    }

    // Validate required parameters
    for param in &parameter_set.parameters {
        if param.required && !bound.named.contains_key(&param.name) {
            // Check if it's satisfied by positional parameter
            if let Some(pos) = param.position {
                if (pos as usize) >= bound.positional.len() {
                    return Err(anyhow!("Required parameter '{}' not provided", param.name));
                }
            } else {
                return Err(anyhow!("Required parameter '{}' not provided", param.name));
            }
        }
    }

    Ok(bound)
}

/// Validate that a value matches the expected parameter type
fn validate_parameter_type(value: &Value, param_type: &ParameterType) -> Result<Value> {
    match (value, param_type) {
        (Value::Str(_), ParameterType::String) => Ok(value.clone()),
        (Value::Int(_), ParameterType::Int) => Ok(value.clone()),
        (Value::Float(_), ParameterType::Float) => Ok(value.clone()),
        (Value::Bool(_), ParameterType::Bool) => Ok(value.clone()),
        (Value::Array(_), ParameterType::Array) => Ok(value.clone()),
        (Value::Record(_), ParameterType::Record) => Ok(value.clone()),

        // Type coercion
        (Value::Str(s), ParameterType::Int) => s
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| anyhow!("Cannot convert '{}' to integer", s)),
        (Value::Str(s), ParameterType::Float) => s
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| anyhow!("Cannot convert '{}' to float", s)),
        (Value::Str(s), ParameterType::Bool) => match s.to_lowercase().as_str() {
            "true" | "yes" | "1" => Ok(Value::Bool(true)),
            "false" | "no" | "0" => Ok(Value::Bool(false)),
            _ => Err(anyhow!("Cannot convert '{}' to boolean", s)),
        },
        (Value::Int(i), ParameterType::Float) => Ok(Value::Float(*i as f64)),

        _ => Err(anyhow!(
            "Parameter type mismatch: expected {:?}, got {:?}",
            param_type,
            value
        )),
    }
}

// =============== PowerShell-style Cmdlets ===============

/// Get-Files: PowerShell-style file listing with rich metadata and proper parameter binding
fn bi_get_files(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    // Define parameter set
    let parameter_set = ParameterSet {
        name: "Get-Files".to_string(),
        parameters: vec![
            Parameter {
                name: "Path".to_string(),
                aliases: vec!["p".to_string()],
                parameter_type: ParameterType::String,
                required: false,
                position: Some(0),
                help_text: "The path to list files from".to_string(),
            },
            Parameter {
                name: "Recurse".to_string(),
                aliases: vec!["r".to_string()],
                parameter_type: ParameterType::Switch,
                required: false,
                position: None,
                help_text: "Recursively list files in subdirectories".to_string(),
            },
            Parameter {
                name: "Filter".to_string(),
                aliases: vec!["f".to_string()],
                parameter_type: ParameterType::String,
                required: false,
                position: None,
                help_text: "Filter files by pattern".to_string(),
            },
        ],
        pipeline_input: Some(PipelineInputType::ByValue(ParameterType::String)),
    };

    // Bind parameters
    let bound = bind_parameters(&parameter_set, args, input)?;

    // Extract parameters
    let path = if let Some(pipeline_path) = &bound.pipeline_input {
        match pipeline_path {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("Get-Files: pipeline input must be a path string")),
        }
    } else if let Some(path_arg) = bound.named.get("Path") {
        match path_arg {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("Get-Files: Path parameter must be a string")),
        }
    } else if !bound.positional.is_empty() {
        match &bound.positional[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("Get-Files: path must be a string")),
        }
    } else {
        ".".to_string()
    };

    let recurse = bound.named.get("Recurse").is_some();
    let filter = bound.named.get("Filter").and_then(|v| {
        if let Value::Str(s) = v {
            Some(s.clone())
        } else {
            None
        }
    });

    let mut files = Vec::new();

    if recurse {
        // Recursive directory traversal
        for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
            if let Ok(metadata) = entry.metadata() {
                let path_str = entry.path().to_string_lossy().to_string();
                let name = entry.file_name().to_string_lossy().to_string();

                // Apply filter if specified
                if let Some(ref pattern) = filter {
                    if !name.contains(pattern) {
                        continue;
                    }
                }

                let mut file_obj = BTreeMap::new();
                file_obj.insert("Name".to_string(), Value::Str(name));
                file_obj.insert("FullName".to_string(), Value::Str(path_str));
                file_obj.insert("IsDirectory".to_string(), Value::Bool(metadata.is_dir()));
                file_obj.insert("Length".to_string(), Value::Int(metadata.len() as i64));

                if let Ok(modified) = metadata.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        file_obj.insert(
                            "LastWriteTime".to_string(),
                            Value::Int(duration.as_secs() as i64),
                        );
                    }
                }

                files.push(Value::Record(file_obj));
            }
        }
    } else {
        // Non-recursive directory listing
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let path_str = entry.path().to_string_lossy().to_string();
            let name = entry.file_name().to_string_lossy().to_string();

            // Apply filter if specified
            if let Some(ref pattern) = filter {
                if !name.contains(pattern) {
                    continue;
                }
            }

            let mut file_obj = BTreeMap::new();
            file_obj.insert("Name".to_string(), Value::Str(name));
            file_obj.insert("FullName".to_string(), Value::Str(path_str));
            file_obj.insert("IsDirectory".to_string(), Value::Bool(metadata.is_dir()));
            file_obj.insert("Length".to_string(), Value::Int(metadata.len() as i64));

            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    file_obj.insert(
                        "LastWriteTime".to_string(),
                        Value::Int(duration.as_secs() as i64),
                    );
                }
            }

            files.push(Value::Record(file_obj));
        }
    }

    Ok(Value::Array(files))
}

/// Get-Content: PowerShell-style file content reading
fn bi_get_content(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let path = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("Get-Content: input must be a path string")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("Get-Content: path must be a string")),
        }
    } else {
        return Err(anyhow!("Get-Content: no path provided"));
    };

    let content = fs::read_to_string(&path)?;

    // Return as array of lines (PowerShell style)
    let lines: Vec<Value> = content
        .lines()
        .map(|line| Value::Str(line.to_string()))
        .collect();

    Ok(Value::Array(lines))
}

/// Select-Object: PowerShell-style property selection
fn bi_select_object(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("Select-Object: requires pipeline input"))?;

    if args.is_empty() {
        return Ok(input);
    }

    match input {
        Value::Array(arr) => {
            let mut results = Vec::new();
            for item in arr {
                results.push(select_properties(&item, &args)?);
            }
            Ok(Value::Array(results))
        }
        _ => Ok(select_properties(&input, &args)?),
    }
}

fn select_properties(value: &Value, properties: &[Value]) -> Result<Value> {
    match value {
        Value::Record(record) => {
            let mut selected = BTreeMap::new();
            for prop in properties {
                if let Value::Str(prop_name) = prop {
                    if let Some(val) = record.get(prop_name) {
                        selected.insert(prop_name.clone(), val.clone());
                    }
                }
            }
            Ok(Value::Record(selected))
        }
        _ => Ok(value.clone()),
    }
}

/// Where-Object: PowerShell-style filtering with enhanced syntax
fn bi_where_object(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("Where-Object: requires pipeline input"))?;

    if args.is_empty() {
        return Err(anyhow!("Where-Object: requires filter condition"));
    }

    // Support both lambda and property-based filtering
    match &args[0] {
        Value::Lambda(_lambda) => {
            // Use existing where logic
            bi_where(args, Some(input), env)
        }
        Value::Str(property) => {
            // PowerShell-style property filtering: where Status -eq "Running"
            if args.len() >= 3 {
                filter_by_property(&input, property, &args[1], &args[2])
            } else {
                Err(anyhow!(
                    "Where-Object: property filtering requires property, operator, and value"
                ))
            }
        }
        _ => Err(anyhow!("Where-Object: invalid filter condition")),
    }
}

fn filter_by_property(
    input: &Value,
    property: &str,
    operator: &Value,
    target: &Value,
) -> Result<Value> {
    let op_str = match operator {
        Value::Str(s) => s.as_str(),
        _ => return Err(anyhow!("Where-Object: operator must be a string")),
    };

    match input {
        Value::Array(arr) => {
            let mut results = Vec::new();
            for item in arr {
                if let Value::Record(record) = item {
                    if let Some(value) = record.get(property) {
                        let matches = match op_str {
                            "-eq" | "==" => value == target,
                            "-ne" | "!=" => value != target,
                            "-gt" | ">" => {
                                compare_values(value, target, std::cmp::Ordering::Greater)
                            }
                            "-lt" | "<" => compare_values(value, target, std::cmp::Ordering::Less),
                            "-ge" | ">=" => {
                                compare_values(value, target, std::cmp::Ordering::Greater)
                                    || value == target
                            }
                            "-le" | "<=" => {
                                compare_values(value, target, std::cmp::Ordering::Less)
                                    || value == target
                            }
                            "-like" => match (value, target) {
                                (Value::Str(v), Value::Str(t)) => v.contains(t),
                                _ => false,
                            },
                            _ => {
                                return Err(anyhow!(
                                    "Where-Object: unsupported operator '{}'",
                                    op_str
                                ));
                            }
                        };

                        if matches {
                            results.push(item.clone());
                        }
                    }
                }
            }
            Ok(Value::Array(results))
        }
        _ => Ok(input.clone()),
    }
}

fn compare_values(a: &Value, b: &Value, expected: std::cmp::Ordering) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y) == expected,
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y) == Some(expected),
        (Value::Int(x), Value::Float(y)) => (*x as f64).partial_cmp(y) == Some(expected),
        (Value::Float(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)) == Some(expected),
        (Value::Str(x), Value::Str(y)) => x.cmp(y) == expected,
        _ => false,
    }
}

/// ForEach-Object: PowerShell-style iteration
fn bi_foreach_object(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // This is essentially the same as map, but with PowerShell naming
    bi_map(args, input, env)
}

/// Sort-Object: PowerShell-style sorting with property support
fn bi_sort_object(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("Sort-Object: requires pipeline input"))?;

    match input {
        Value::Array(mut arr) => {
            if args.is_empty() {
                // Sort by value
                arr.sort_by(|a, b| compare_values_for_sort(a, b));
            } else if let Value::Str(property) = &args[0] {
                // Sort by property
                arr.sort_by(|a, b| {
                    let a_val = get_property_value(a, property);
                    let b_val = get_property_value(b, property);
                    compare_values_for_sort(&a_val, &b_val)
                });
            }
            Ok(Value::Array(arr))
        }
        _ => Ok(input),
    }
}

fn get_property_value(value: &Value, property: &str) -> Value {
    match value {
        Value::Record(record) => record
            .get(property)
            .cloned()
            .unwrap_or(Value::Str("".to_string())),
        _ => value.clone(),
    }
}

fn compare_values_for_sort(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Int(x), Value::Float(y)) => (*x as f64)
            .partial_cmp(y)
            .unwrap_or(std::cmp::Ordering::Equal),
        (Value::Float(x), Value::Int(y)) => x
            .partial_cmp(&(*y as f64))
            .unwrap_or(std::cmp::Ordering::Equal),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => std::cmp::Ordering::Equal,
    }
}

/// Group-Object: PowerShell-style grouping
fn bi_group_object(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("Group-Object: requires pipeline input"))?;

    if args.is_empty() {
        return Err(anyhow!("Group-Object: requires property name"));
    }

    let property = match &args[0] {
        Value::Str(s) => s,
        _ => return Err(anyhow!("Group-Object: property name must be a string")),
    };

    match input {
        Value::Array(arr) => {
            let mut groups = BTreeMap::new();

            for item in arr {
                let key = get_property_value(&item, property);
                let key_str = value_to_string(&key);
                groups.entry(key_str).or_insert_with(Vec::new).push(item);
            }

            let mut result = Vec::new();
            for (key, items) in groups {
                let mut group = BTreeMap::new();
                group.insert("Name".to_string(), Value::Str(key));
                group.insert("Count".to_string(), Value::Int(items.len() as i64));
                group.insert("Group".to_string(), Value::Array(items));
                result.push(Value::Record(group));
            }

            Ok(Value::Array(result))
        }
        _ => Err(anyhow!("Group-Object: input must be an array")),
    }
}

/// Measure-Object: PowerShell-style measurement and statistics
fn bi_measure_object(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("Measure-Object: requires pipeline input"))?;

    match input {
        Value::Array(arr) => {
            let mut result = BTreeMap::new();
            result.insert("Count".to_string(), Value::Int(arr.len() as i64));

            // If property specified, calculate statistics
            if !args.is_empty() {
                if let Value::Str(property) = &args[0] {
                    let values: Vec<f64> = arr
                        .iter()
                        .filter_map(|item| {
                            let prop_val = get_property_value(item, property);
                            match prop_val {
                                Value::Int(i) => Some(i as f64),
                                Value::Float(f) => Some(f),
                                _ => None,
                            }
                        })
                        .collect();

                    if !values.is_empty() {
                        let sum: f64 = values.iter().sum();
                        let avg = sum / values.len() as f64;
                        let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                        let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));

                        result.insert("Sum".to_string(), Value::Float(sum));
                        result.insert("Average".to_string(), Value::Float(avg));
                        result.insert("Minimum".to_string(), Value::Float(min));
                        result.insert("Maximum".to_string(), Value::Float(max));
                    }
                }
            }

            Ok(Value::Record(result))
        }
        _ => {
            let mut result = BTreeMap::new();
            result.insert("Count".to_string(), Value::Int(1));
            Ok(Value::Record(result))
        }
    }
}

// =============== Nushell-style Data Commands ===============

/// from-json: Parse JSON into structured data
fn bi_from_json(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let json_str = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("from-json: input must be a JSON string")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("from-json: argument must be a JSON string")),
        }
    } else {
        return Err(anyhow!("from-json: no JSON input provided"));
    };

    let parsed: serde_json::Value = serde_json::from_str(&json_str)?;
    Ok(json_to_value(parsed))
}

/// to-json: Convert structured data to JSON
fn bi_to_json(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let value = input.ok_or_else(|| anyhow!("to-json: requires pipeline input"))?;

    let pretty = args
        .get(0)
        .and_then(|arg| match arg {
            Value::Bool(b) => Some(*b),
            Value::Str(s) => Some(s == "pretty" || s == "true"),
            _ => None,
        })
        .unwrap_or(false);

    let json_val = value_to_json(value);
    let json_str = if pretty {
        serde_json::to_string_pretty(&json_val)?
    } else {
        serde_json::to_string(&json_val)?
    };

    Ok(Value::Str(json_str))
}

fn json_to_value(json: serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Str("null".to_string()),
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Str(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(arr) => Value::Array(arr.into_iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => {
            let mut map = BTreeMap::new();
            for (k, v) in obj {
                map.insert(k, json_to_value(v));
            }
            Value::Record(map)
        }
    }
}

fn value_to_json(value: Value) -> serde_json::Value {
    match value {
        Value::Str(s) => serde_json::Value::String(s),
        Value::Int(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        Value::Float(f) => serde_json::Value::Number(
            serde_json::Number::from_f64(f).unwrap_or_else(|| serde_json::Number::from(0)),
        ),
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Array(arr) => serde_json::Value::Array(arr.into_iter().map(value_to_json).collect()),
        Value::Record(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k, value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::String(value_to_string(&value)),
    }
}

/// from-csv: Parse CSV into structured data
fn bi_from_csv(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let csv_str = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("from-csv: input must be a CSV string")),
        }
    } else {
        return Err(anyhow!("from-csv: requires CSV input"));
    };

    let mut reader = csv::Reader::from_reader(csv_str.as_bytes());
    let headers: Vec<String> = reader.headers()?.iter().map(|h| h.to_string()).collect();

    let mut results = Vec::new();
    for record in reader.records() {
        let record = record?;
        let mut row = BTreeMap::new();

        for (i, field) in record.iter().enumerate() {
            if let Some(header) = headers.get(i) {
                // Try to parse as number, otherwise keep as string
                let value = if let Ok(int_val) = field.parse::<i64>() {
                    Value::Int(int_val)
                } else if let Ok(float_val) = field.parse::<f64>() {
                    Value::Float(float_val)
                } else if let Ok(bool_val) = field.parse::<bool>() {
                    Value::Bool(bool_val)
                } else {
                    Value::Str(field.to_string())
                };
                row.insert(header.clone(), value);
            }
        }
        results.push(Value::Record(row));
    }

    Ok(Value::Array(results))
}

/// to-csv: Convert structured data to CSV
fn bi_to_csv(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("to-csv: requires pipeline input"))?;

    match input {
        Value::Array(arr) => {
            if arr.is_empty() {
                return Ok(Value::Str(String::new()));
            }

            // Get headers from first record
            let headers = if let Value::Record(first_record) = &arr[0] {
                first_record.keys().cloned().collect::<Vec<_>>()
            } else {
                return Err(anyhow!("to-csv: input must be array of records"));
            };

            let mut csv_output = String::new();

            // Write headers
            csv_output.push_str(&headers.join(","));
            csv_output.push('\n');

            // Write data rows
            for item in arr {
                if let Value::Record(record) = item {
                    let row: Vec<String> = headers
                        .iter()
                        .map(|header| {
                            record
                                .get(header)
                                .map(|v| csv_escape(&value_to_string(v)))
                                .unwrap_or_default()
                        })
                        .collect();
                    csv_output.push_str(&row.join(","));
                    csv_output.push('\n');
                }
            }

            Ok(Value::Str(csv_output))
        }
        _ => Err(anyhow!("to-csv: input must be an array of records")),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// from-yaml: Parse YAML into structured data (simplified)
fn bi_from_yaml(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    // Simplified YAML parsing - in a real implementation, use serde_yaml
    let yaml_str = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("from-yaml: input must be a YAML string")),
        }
    } else {
        return Err(anyhow!("from-yaml: requires YAML input"));
    };

    // For now, treat simple key-value pairs
    let mut result = BTreeMap::new();
    for line in yaml_str.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim();

            let parsed_value = if let Ok(int_val) = value.parse::<i64>() {
                Value::Int(int_val)
            } else if let Ok(float_val) = value.parse::<f64>() {
                Value::Float(float_val)
            } else if let Ok(bool_val) = value.parse::<bool>() {
                Value::Bool(bool_val)
            } else {
                Value::Str(value.to_string())
            };

            result.insert(key, parsed_value);
        }
    }

    Ok(Value::Record(result))
}

/// to-yaml: Convert structured data to YAML (simplified)
fn bi_to_yaml(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("to-yaml: requires pipeline input"))?;

    match input {
        Value::Record(record) => {
            let mut yaml_output = String::new();
            for (key, value) in record {
                yaml_output.push_str(&format!("{}: {}\n", key, value_to_string(&value)));
            }
            Ok(Value::Str(yaml_output))
        }
        _ => Ok(Value::Str(format!("value: {}\n", value_to_string(&input)))),
    }
}

/// columns: Get column names from structured data
fn bi_columns(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("columns: requires pipeline input"))?;

    match input {
        Value::Array(arr) => {
            if let Some(Value::Record(first)) = arr.first() {
                let columns: Vec<Value> = first.keys().map(|k| Value::Str(k.clone())).collect();
                Ok(Value::Array(columns))
            } else {
                Ok(Value::Array(Vec::new()))
            }
        }
        Value::Record(record) => {
            let columns: Vec<Value> = record.keys().map(|k| Value::Str(k.clone())).collect();
            Ok(Value::Array(columns))
        }
        _ => Ok(Value::Array(Vec::new())),
    }
}

/// describe: Get type and structure information about data
fn bi_describe(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("describe: requires pipeline input"))?;

    let mut description = BTreeMap::new();

    match &input {
        Value::Str(s) => {
            description.insert("type".to_string(), Value::Str("string".to_string()));
            description.insert("length".to_string(), Value::Int(s.len() as i64));
        }
        Value::Int(_) => {
            description.insert("type".to_string(), Value::Str("integer".to_string()));
        }
        Value::Float(_) => {
            description.insert("type".to_string(), Value::Str("float".to_string()));
        }
        Value::Bool(_) => {
            description.insert("type".to_string(), Value::Str("boolean".to_string()));
        }
        Value::Array(arr) => {
            description.insert("type".to_string(), Value::Str("array".to_string()));
            description.insert("length".to_string(), Value::Int(arr.len() as i64));

            if !arr.is_empty() {
                let first_type = match &arr[0] {
                    Value::Str(_) => "string",
                    Value::Int(_) => "integer",
                    Value::Float(_) => "float",
                    Value::Bool(_) => "boolean",
                    Value::Array(_) => "array",
                    Value::Record(_) => "record",
                    _ => "unknown",
                };
                description.insert(
                    "element_type".to_string(),
                    Value::Str(first_type.to_string()),
                );
            }
        }
        Value::Record(record) => {
            description.insert("type".to_string(), Value::Str("record".to_string()));
            description.insert("fields".to_string(), Value::Int(record.len() as i64));

            let field_names: Vec<Value> = record.keys().map(|k| Value::Str(k.clone())).collect();
            description.insert("field_names".to_string(), Value::Array(field_names));
        }
        _ => {
            description.insert("type".to_string(), Value::Str("unknown".to_string()));
        }
    }

    Ok(Value::Record(description))
}

// =============== AI-Enhanced Commands ===============

/// ai: Direct AI query with optional multi-modal support
/// Usage:
///   ai("prompt")                              - Simple text query
///   ai("prompt", {images: ["path1", "path2"]}) - With images
///   ai("prompt", {audio: ["path"]})           - With audio
///   ai("prompt", {video: ["path"]})           - With video
///   ai("prompt", {model: "openai:gpt-4o"})    - Specify model
fn bi_ai(args: Vec<Value>, input: Option<Value>, _env: &mut Env) -> Result<Value> {
    // SECURITY: Rate limit AI calls
    check_rate_limit("bi_ai", 30, Duration::from_secs(60)).context("AI rate limit exceeded")?;

    // Get prompt from input or first argument
    let prompt = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("ai: input must be a prompt string")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("ai: prompt must be a string")),
        }
    } else {
        return Err(anyhow!("ai: requires a prompt string"));
    };

    // SECURITY: Validate AI prompt for injection
    let validated_prompt = validate_ai_prompt(&prompt).context("ai: prompt validation failed")?;

    // Check for optional config record (second argument)
    let config = args.get(1).and_then(|v| {
        if let Value::Record(r) = v {
            Some(r.clone())
        } else {
            None
        }
    });

    // Extract multi-modal content if present
    let images = config
        .as_ref()
        .and_then(|c| extract_string_array(c, "images"));
    let audio = config
        .as_ref()
        .and_then(|c| extract_string_array(c, "audio"));
    let video = config
        .as_ref()
        .and_then(|c| extract_string_array(c, "video"));
    let _model = config.as_ref().and_then(|c| {
        c.get("model").and_then(|v| {
            if let Value::Str(s) = v {
                Some(s.clone())
            } else {
                None
            }
        })
    });

    // Determine if this is a multi-modal request
    let has_media = images.is_some() || audio.is_some() || video.is_some();

    if has_media {
        // Multi-modal request
        let mut content_parts = vec![crate::ai::MultiModalContent {
            text: Some(validated_prompt),
            image_url: None,
            audio_url: None,
            video_url: None,
            image_data: None,
            audio_data: None,
            video_data: None,
        }];

        // Load and encode images
        if let Some(img_paths) = images {
            for path in img_paths {
                // SECURITY: Validate path
                let validated_path = validate_read_path(&path)
                    .context(format!("ai: invalid image path: {}", path))?;

                // Read and base64 encode the image
                let data = fs::read(&validated_path).context(format!(
                    "ai: failed to read image: {}",
                    validated_path.display()
                ))?;

                // SECURITY: Check file size
                check_file_size_limit(data.len() as u64).context("ai: image file too large")?;

                use base64::{engine::general_purpose::STANDARD, Engine as _};
                let encoded = STANDARD.encode(&data);

                content_parts.push(crate::ai::MultiModalContent {
                    text: None,
                    image_url: None,
                    audio_url: None,
                    video_url: None,
                    image_data: Some(encoded),
                    audio_data: None,
                    video_data: None,
                });
            }
        }

        // Load and encode audio
        if let Some(audio_paths) = audio {
            for path in audio_paths {
                let validated_path = validate_read_path(&path)
                    .context(format!("ai: invalid audio path: {}", path))?;

                let data = fs::read(&validated_path).context(format!(
                    "ai: failed to read audio: {}",
                    validated_path.display()
                ))?;

                // SECURITY: Check file size
                check_file_size_limit(data.len() as u64).context("ai: audio file too large")?;

                use base64::{engine::general_purpose::STANDARD, Engine as _};
                let encoded = STANDARD.encode(&data);

                content_parts.push(crate::ai::MultiModalContent {
                    text: None,
                    image_url: None,
                    audio_url: None,
                    video_url: None,
                    image_data: None,
                    audio_data: Some(encoded),
                    video_data: None,
                });
            }
        }

        // Load and encode video
        if let Some(video_paths) = video {
            for path in video_paths {
                let validated_path = validate_read_path(&path)
                    .context(format!("ai: invalid video path: {}", path))?;

                let data = fs::read(&validated_path).context(format!(
                    "ai: failed to read video: {}",
                    validated_path.display()
                ))?;

                // SECURITY: Check file size
                check_file_size_limit(data.len() as u64).context("ai: video file too large")?;

                use base64::{engine::general_purpose::STANDARD, Engine as _};
                let encoded = STANDARD.encode(&data);

                content_parts.push(crate::ai::MultiModalContent {
                    text: None,
                    image_url: None,
                    audio_url: None,
                    video_url: None,
                    image_data: None,
                    audio_data: None,
                    video_data: Some(encoded),
                });
            }
        }

        let message = crate::ai::MultiModalMessage {
            role: "user".to_string(),
            content: content_parts,
        };

        let response = crate::ai::complete_multimodal_sync(&[message])?;

        Ok(Value::Str(response))
    } else {
        // Simple text-only request
        let response = crate::ai::complete_sync_router(&validated_prompt)?;

        Ok(Value::Str(response))
    }
}

/// Helper to extract a string array from a record field
fn extract_string_array(record: &BTreeMap<String, Value>, key: &str) -> Option<Vec<String>> {
    record.get(key).and_then(|v| {
        if let Value::Array(arr) = v {
            let strings: Vec<String> = arr
                .iter()
                .filter_map(|item| {
                    if let Value::Str(s) = item {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect();
            if strings.is_empty() {
                None
            } else {
                Some(strings)
            }
        } else {
            None
        }
    })
}

/// ai-suggest: Get AI-powered command suggestions
fn bi_ai_suggest(args: Vec<Value>, input: Option<Value>, _env: &mut Env) -> Result<Value> {
    let query = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("ai-suggest: input must be a query string")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("ai-suggest: query must be a string")),
        }
    } else {
        return Err(anyhow!("ai-suggest: requires query"));
    };

    // For now, provide rule-based suggestions
    let suggestions = get_command_suggestions(&query);
    Ok(Value::Array(suggestions))
}

fn get_command_suggestions(query: &str) -> Vec<Value> {
    let query_lower = query.to_lowercase();
    let mut suggestions = Vec::new();

    if query_lower.contains("list") || query_lower.contains("files") {
        suggestions.push(Value::Str("ls or Get-Files for listing files".to_string()));
        suggestions.push(Value::Str(
            "find . \"*.ext\" for finding specific files".to_string(),
        ));
    }

    if query_lower.contains("read") || query_lower.contains("content") {
        suggestions.push(Value::Str(
            "cat filename or Get-Content filename".to_string(),
        ));
        suggestions.push(Value::Str("head filename for first few lines".to_string()));
    }

    if query_lower.contains("filter") || query_lower.contains("search") {
        suggestions.push(Value::Str("grep \"pattern\" for text search".to_string()));
        suggestions.push(Value::Str(
            "where fn(x) => condition for filtering".to_string(),
        ));
        suggestions.push(Value::Str("Where-Object property -eq value".to_string()));
    }

    if query_lower.contains("sort") {
        suggestions.push(Value::Str("sort for basic sorting".to_string()));
        suggestions.push(Value::Str(
            "Sort-Object property for property-based sorting".to_string(),
        ));
    }

    if query_lower.contains("json") {
        suggestions.push(Value::Str("from-json for parsing JSON".to_string()));
        suggestions.push(Value::Str("to-json for converting to JSON".to_string()));
    }

    if suggestions.is_empty() {
        suggestions.push(Value::Str(
            "Try: ls, cat, grep, sort, map, where".to_string(),
        ));
        suggestions.push(Value::Str(
            "Use 'help' for complete command list".to_string(),
        ));
    }

    suggestions
}

/// ai-explain: Get AI-powered explanations of commands or errors
fn bi_ai_explain(args: Vec<Value>, input: Option<Value>, _env: &mut Env) -> Result<Value> {
    let subject = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("ai-explain: input must be a string to explain")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("ai-explain: subject must be a string")),
        }
    } else {
        return Err(anyhow!("ai-explain: requires something to explain"));
    };

    let explanation = generate_explanation(&subject);
    Ok(Value::Str(explanation))
}

fn generate_explanation(subject: &str) -> String {
    let subject_lower = subject.to_lowercase();

    if subject_lower.contains("error") || subject_lower.contains("failed") {
        format!(
            "🔍 Error Analysis: {}\n\n\
            Common causes:\n\
            • Check file paths and permissions\n\
            • Verify command syntax\n\
            • Ensure required arguments are provided\n\
            • Try 'help command_name' for usage info\n\n\
            💡 Use 'ai-suggest \"how to...\"' for alternative approaches",
            subject
        )
    } else if subject_lower.starts_with("ls") || subject_lower.contains("get-files") {
        "📁 ls / Get-Files: Lists files and directories\n\
        • ls . - list current directory\n\
        • ls path - list specific directory\n\
        • Get-Files returns rich objects with metadata\n\
        • Pipe to 'where' or 'select' for filtering"
            .to_string()
    } else if subject_lower.starts_with("cat") || subject_lower.contains("get-content") {
        "📄 cat / Get-Content: Reads file contents\n\
        • cat filename - display entire file\n\
        • Get-Content returns array of lines\n\
        • Pipe to 'head' or 'tail' to limit output\n\
        • Use with grep for searching"
            .to_string()
    } else if subject_lower.contains("pipe") || subject_lower.contains("|") {
        "🔗 Pipelines: Connect commands together\n\
        • data | command - pass data to next command\n\
        • Supports structured data (objects, arrays)\n\
        • Each command processes and transforms data\n\
        • Example: ls . | where fn(f) => !f.is_dir | head 5"
            .to_string()
    } else {
        format!(
            "💭 About: {}\n\n\
            This appears to be a command or concept in AetherShell.\n\
            • Try running it to see what happens\n\
            • Use 'help' for general assistance\n\
            • Check syntax with similar commands\n\
            • Use 'ai-suggest' for alternative approaches",
            subject
        )
    }
}

/// ai-complete: Get AI-powered command completion
fn bi_ai_complete(args: Vec<Value>, input: Option<Value>, _env: &mut Env) -> Result<Value> {
    let partial = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => {
                return Err(anyhow!(
                    "ai-complete: input must be a partial command string"
                ));
            }
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("ai-complete: partial command must be a string")),
        }
    } else {
        return Err(anyhow!("ai-complete: requires partial command"));
    };

    let completions = get_smart_completions(&partial);
    Ok(Value::Array(completions))
}

fn get_smart_completions(partial: &str) -> Vec<Value> {
    let mut completions = Vec::new();

    let builtins = [
        "ls",
        "cat",
        "head",
        "tail",
        "grep",
        "find",
        "sort",
        "uniq",
        "wc",
        "pwd",
        "map",
        "where",
        "reduce",
        "take",
        "print",
        "echo",
        "Get-Files",
        "Get-Content",
        "Select-Object",
        "Where-Object",
        "Sort-Object",
        "from-json",
        "to-json",
        "from-csv",
        "to-csv",
        "describe",
        "columns",
        "ai-suggest",
        "ai-explain",
        "ai-complete",
        "ai-fix",
    ];

    // Basic command completion
    for builtin in &builtins {
        if builtin.to_lowercase().starts_with(&partial.to_lowercase()) {
            completions.push(Value::Str(builtin.to_string()));
        }
    }

    // Pipeline completions
    if partial.ends_with("| ") {
        for cmd in &["map", "where", "select", "sort", "head", "tail", "grep"] {
            completions.push(Value::Str(format!("{}{}", partial, cmd)));
        }
    }

    // File path completions (simplified)
    if partial.contains("\"") || partial.contains("/") || partial.contains("\\") {
        completions.push(Value::Str("./".to_string()));
        completions.push(Value::Str("../".to_string()));
        completions.push(Value::Str("README.md".to_string()));
    }

    completions
}

/// ai-fix: Get AI-powered error fixes and suggestions
fn bi_ai_fix(args: Vec<Value>, input: Option<Value>, _env: &mut Env) -> Result<Value> {
    let error_msg = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("ai-fix: input must be an error message string")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("ai-fix: error message must be a string")),
        }
    } else {
        return Err(anyhow!("ai-fix: requires error message"));
    };

    let fix_suggestion = generate_fix_suggestion(&error_msg);
    Ok(Value::Str(fix_suggestion))
}

fn generate_fix_suggestion(error: &str) -> String {
    let error_lower = error.to_lowercase();

    if error_lower.contains("unknown builtin") {
        "🔧 Fix: Command not found\n\
        1. Check spelling: ls, cat, grep, etc.\n\
        2. Try similar commands: ai-suggest \"what command...\"\n\
        3. Use 'help' to see available commands\n\
        4. For PowerShell users: Get-Files instead of Get-ChildItem"
            .to_string()
    } else if error_lower.contains("requires array input") {
        "🔧 Fix: Type mismatch\n\
        1. Wrap single value in array: [value] | command\n\
        2. Use array-producing commands first: ls | command\n\
        3. Check if input data is structured correctly\n\
        4. Try 'describe' to see data type"
            .to_string()
    } else if error_lower.contains("no such file") || error_lower.contains("file not found") {
        "🔧 Fix: File not found\n\
        1. Check current directory: pwd\n\
        2. List available files: ls or Get-Files\n\
        3. Use absolute path: /full/path/to/file\n\
        4. Search for file: find . \"filename*\""
            .to_string()
    } else if error_lower.contains("permission denied") {
        "🔧 Fix: Permission denied\n\
        1. Check file permissions: ls -la (on Unix)\n\
        2. Run with appropriate privileges\n\
        3. Verify file ownership\n\
        4. Use different file location"
            .to_string()
    } else if error_lower.contains("syntax error") || error_lower.contains("parse") {
        "🔧 Fix: Syntax error\n\
        1. Check command syntax: help command_name\n\
        2. Verify parentheses and quotes are balanced\n\
        3. Use proper pipeline syntax: cmd1 | cmd2\n\
        4. Try simpler version first"
            .to_string()
    } else {
        format!(
            "🔧 General Fix Suggestions:\n\
            Error: {}\n\n\
            1. Check command spelling and syntax\n\
            2. Verify input data types with 'describe'\n\
            3. Use 'help' for command documentation\n\
            4. Try 'ai-suggest' for alternative approaches\n\
            5. Break complex commands into simpler steps",
            error
        )
    }
}

// ============ Option Type Constructors ============

/// Some(value) - Creates an Option variant containing a value
/// Returns: Record with _tag="Some" and _value=<arg>
fn bi_some(args: Vec<Value>) -> Result<Value> {
    if args.len() != 1 {
        return Err(anyhow!(
            "Some expects exactly 1 argument, got {}",
            args.len()
        ));
    }

    let mut record = BTreeMap::new();
    record.insert("_tag".to_string(), Value::Str("Some".to_string()));
    record.insert("_value".to_string(), args[0].clone());

    Ok(Value::Record(record))
}

/// None - Creates an Option variant representing no value
/// Returns: Record with _tag="None"
fn bi_none() -> Result<Value> {
    let mut record = BTreeMap::new();
    record.insert("_tag".to_string(), Value::Str("None".to_string()));

    Ok(Value::Record(record))
}

// ============ AI Backend Detection ============

/// ai_backends() - Detect and list all available AI backends
/// Returns: Array of records with backend information
///
/// Example:
///   ai_backends() | select(["name", "available", "models"])
fn bi_ai_backends(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let backends = crate::ai::detect_available_backends();

    let records: Vec<Value> = backends
        .into_iter()
        .map(|backend| {
            let mut record = BTreeMap::new();
            record.insert("name".to_string(), Value::Str(backend.name));
            record.insert(
                "provider".to_string(),
                Value::Str(format!("{:?}", backend.provider)),
            );
            record.insert("endpoint".to_string(), Value::Str(backend.endpoint));
            record.insert("available".to_string(), Value::Bool(backend.available));

            let models: Vec<Value> = backend.models.into_iter().map(Value::Str).collect();
            record.insert("models".to_string(), Value::Array(models));

            Value::Record(record)
        })
        .collect();

    Ok(Value::Array(records))
}

/// ai_detect() - Automatically detect and return the best available backend
/// Returns: String with the model URI (e.g., "ollama:llama3" or "vllm:model")
/// Returns empty string if no backend is available.
///
/// Example:
///   let backend = ai_detect()
///   if backend != "" { ai(backend, "Hello!") }
fn bi_ai_detect(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    match crate::ai::auto_select_backend() {
        Some(uri) => Ok(Value::Str(uri)),
        None => Ok(Value::Str(String::new())),
    }
}

// ========== MCP Server Detection Functions ==========

/// mcp_servers() - Detect all available MCP servers
/// Returns: Array of records with MCP server information
///
/// Each record contains:
///   - name: String - Server name (e.g., "filesystem", "git")
///   - endpoint: String - Server endpoint URL
///   - available: Bool - Whether server is reachable
///   - tools: Array[String] - List of available tool names
///
/// Example:
///   let servers = mcp_servers()
///   servers | foreach(fn(s) => print(s.name + ": " + len(s.tools) + " tools"))
fn bi_mcp_servers(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let servers = crate::ai::detect_mcp_servers();

    let records: Vec<Value> = servers
        .into_iter()
        .map(|server| {
            let mut record = BTreeMap::new();
            record.insert("name".to_string(), Value::Str(server.name));
            record.insert("endpoint".to_string(), Value::Str(server.endpoint));
            record.insert("available".to_string(), Value::Bool(server.available));

            let tools: Vec<Value> = server.tools.into_iter().map(Value::Str).collect();
            record.insert("tools".to_string(), Value::Array(tools));

            Value::Record(record)
        })
        .collect();

    Ok(Value::Array(records))
}

/// mcp_detect(endpoint?) - Detect MCP server at specific endpoint or find first available
/// Args:
///   - endpoint (optional): String - Specific endpoint to check
/// Returns: Record with MCP server info, or null if not found
///
/// Example:
///   let fs_server = mcp_detect("http://localhost:3001")
///   let any_server = mcp_detect()
fn bi_mcp_detect(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if !args.is_empty() {
        // Check specific endpoint
        if let Value::Str(endpoint) = &args[0] {
            let servers = crate::ai::detect_mcp_servers();
            if let Some(server) = servers.iter().find(|s| &s.endpoint == endpoint) {
                let mut record = BTreeMap::new();
                record.insert("name".to_string(), Value::Str(server.name.clone()));
                record.insert("endpoint".to_string(), Value::Str(server.endpoint.clone()));
                record.insert("available".to_string(), Value::Bool(server.available));

                let tools: Vec<Value> =
                    server.tools.iter().map(|t| Value::Str(t.clone())).collect();
                record.insert("tools".to_string(), Value::Array(tools));

                return Ok(Value::Record(record));
            }
        }
        Ok(Value::Null)
    } else {
        // Return first available MCP server
        let servers = crate::ai::detect_mcp_servers();
        if let Some(server) = servers.first() {
            let mut record = BTreeMap::new();
            record.insert("name".to_string(), Value::Str(server.name.clone()));
            record.insert("endpoint".to_string(), Value::Str(server.endpoint.clone()));
            record.insert("available".to_string(), Value::Bool(server.available));

            let tools: Vec<Value> = server.tools.iter().map(|t| Value::Str(t.clone())).collect();
            record.insert("tools".to_string(), Value::Array(tools));

            Ok(Value::Record(record))
        } else {
            Ok(Value::Null)
        }
    }
}

/// mcp_cache_clear() - Clear the MCP detection cache
/// Returns: Bool - true if cache was cleared successfully
///
/// Example:
///   mcp_cache_clear()
///   let servers = mcp_servers()  # This will re-detect servers
fn bi_mcp_cache_clear(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let _ = crate::ai::clear_mcp_cache();
    Ok(Value::Bool(true))
}

/// mcp_cache_status() - Get MCP cache status information
/// Returns: Record with cache status details
///
/// Example:
///   let status = mcp_cache_status()
///   print("Cache hit rate: " + status.hit_rate)
fn bi_mcp_cache_status(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let mut record = BTreeMap::new();

    // For now, just return basic status - we could extend this later with hit/miss counters
    record.insert("enabled".to_string(), Value::Bool(true));
    record.insert("ttl_seconds".to_string(), Value::Int(30)); // Default TTL

    // Check if cache has data
    let has_cached_data = {
        match crate::ai::MCP_CACHE.lock() {
            Ok(cache_guard) => cache_guard.is_some(),
            Err(_) => {
                tracing::warn!("Failed to acquire MCP cache lock for status check");
                false
            }
        }
    };
    record.insert("has_cached_data".to_string(), Value::Bool(has_cached_data));

    Ok(Value::Record(record))
}

// ==================== MCP Server/Client Functions ====================

/// Helper function to map category strings to ToolCategory enum
fn parse_tool_category(s: &str) -> Option<crate::os_tools::ToolCategory> {
    use crate::os_tools::ToolCategory;
    match s.to_lowercase().as_str() {
        "filesystem" | "file_system" => Some(ToolCategory::FileSystem),
        "textprocessing" | "text_processing" | "text" => Some(ToolCategory::TextProcessing),
        "networktools" | "network_tools" | "network" => Some(ToolCategory::NetworkTools),
        "systeminfo" | "system_info" | "system" => Some(ToolCategory::SystemInfo),
        "processmanagement" | "process_management" | "process" => {
            Some(ToolCategory::ProcessManagement)
        }
        "archives" | "archive" => Some(ToolCategory::Archives),
        "searchtools" | "search_tools" | "search" => Some(ToolCategory::SearchTools),
        "monitoring" => Some(ToolCategory::Monitoring),
        "development" | "dev" => Some(ToolCategory::Development),
        "media" => Some(ToolCategory::Media),
        "security" => Some(ToolCategory::Security),
        "utilities" | "util" => Some(ToolCategory::Utilities),
        "webtools" | "web_tools" | "web" => Some(ToolCategory::WebTools),
        "cybersecurity" | "cyber_security" | "cyber" => Some(ToolCategory::CyberSecurity),
        "reconnaissance" | "recon" => Some(ToolCategory::Reconnaissance),
        "forensics" => Some(ToolCategory::Forensics),
        "cryptography" | "crypto" => Some(ToolCategory::Cryptography),
        "cloudaws" | "cloud_aws" | "aws" => Some(ToolCategory::CloudAWS),
        "cloudazure" | "cloud_azure" | "azure" => Some(ToolCategory::CloudAzure),
        "cloudgcp" | "cloud_gcp" | "gcp" => Some(ToolCategory::CloudGCP),
        "kubernetes" | "k8s" => Some(ToolCategory::Kubernetes),
        "containers" | "docker" | "podman" => Some(ToolCategory::Containers),
        "infrastructure" | "terraform" | "ansible" => Some(ToolCategory::Infrastructure),
        "database" | "db" => Some(ToolCategory::Database),
        "dataprocessing" | "data_processing" | "data" => Some(ToolCategory::DataProcessing),
        "machinelearning" | "machine_learning" | "ml" => Some(ToolCategory::MachineLearning),
        _ => None,
    }
}

/// mcp_server(config?) - Create a local MCP server instance for AI tool access
/// Args:
///   - config (optional): Record with configuration options
///     - max_safety_level: String - "safe", "caution", "dangerous", "critical" (default: "caution")
///     - allow_admin: Bool - Allow admin tools (default: false)
///     - categories: Array[String] - Limit to specific tool categories (default: all)
///     - blocked_tools: Array[String] - Tools to block (default: [])
/// Returns: Record with MCP server information
///
/// Example:
///   let server = mcp_server()
///   let safe_server = mcp_server({ max_safety_level: "safe" })
///   let k8s_server = mcp_server({ categories: ["kubernetes", "containers"] })
fn bi_mcp_server(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    use crate::mcp::{McpConfig, McpServer};
    use crate::os_tools::SafetyLevel;

    // Parse config from args
    let mut config = McpConfig::default();
    let mut allowed_categories = None;

    if !args.is_empty() {
        if let Value::Record(rec) = &args[0] {
            // Parse safety level
            if let Some(Value::Str(level)) = rec.get("max_safety_level") {
                config.max_safety_level = match level.to_lowercase().as_str() {
                    "safe" => SafetyLevel::Safe,
                    "caution" => SafetyLevel::Caution,
                    "dangerous" => SafetyLevel::Dangerous,
                    "critical" => SafetyLevel::Critical,
                    _ => SafetyLevel::Caution,
                };
            }

            // Parse allow_admin
            if let Some(Value::Bool(b)) = rec.get("allow_admin") {
                config.allow_admin_tools = *b;
            }

            // Parse blocked_tools
            if let Some(Value::Array(arr)) = rec.get("blocked_tools") {
                config.blocked_tools = arr
                    .iter()
                    .filter_map(|v| {
                        if let Value::Str(s) = v {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
            }

            // Parse categories
            if let Some(Value::Array(arr)) = rec.get("categories") {
                let cats: Vec<_> = arr
                    .iter()
                    .filter_map(|v| {
                        if let Value::Str(s) = v {
                            parse_tool_category(s)
                        } else {
                            None
                        }
                    })
                    .collect();
                if !cats.is_empty() {
                    allowed_categories = Some(cats);
                }
            }
        }
    }

    // Set allowed_categories in config
    config.allowed_categories = allowed_categories;

    // Create MCP server
    let server = McpServer::with_config(config);
    let init_result = server.initialize();

    // Return server info as record
    let mut record = BTreeMap::new();
    record.insert(
        "protocol_version".to_string(),
        Value::Str(init_result.protocol_version),
    );
    record.insert(
        "server_name".to_string(),
        Value::Str(init_result.server_info.name),
    );
    record.insert(
        "server_version".to_string(),
        Value::Str(init_result.server_info.version),
    );
    record.insert(
        "tool_count".to_string(),
        Value::Int(server.list_tools().len() as i64),
    );

    let capabilities: Vec<Value> = vec![
        Value::Str("tools".to_string()),
        Value::Str("resources".to_string()),
        Value::Str("prompts".to_string()),
    ];
    record.insert("capabilities".to_string(), Value::Array(capabilities));

    Ok(Value::Record(record))
}

/// mcp_tools(config?) - List available MCP tools with their schemas
/// Args:
///   - config (optional): Record with filter options
///     - category: String - Filter by category
///     - search: String - Search by name/description
///     - safety_level: String - Max safety level
/// Returns: Array of tool records with name, description, and input schema
///
/// Example:
///   let all_tools = mcp_tools()
///   let k8s_tools = mcp_tools({ category: "kubernetes" })
///   let git_tools = mcp_tools({ search: "git" })
fn bi_mcp_tools(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    use crate::mcp::{McpConfig, McpServer};
    use crate::os_tools::SafetyLevel;

    // Parse filters
    let mut config = McpConfig::default();
    let mut search_filter: Option<String> = None;

    if !args.is_empty() {
        if let Value::Record(rec) = &args[0] {
            // Parse category filter
            if let Some(Value::Str(cat_str)) = rec.get("category") {
                if let Some(cat) = parse_tool_category(cat_str) {
                    config.allowed_categories = Some(vec![cat]);
                }
            }

            // Parse search filter
            if let Some(Value::Str(s)) = rec.get("search") {
                search_filter = Some(s.clone());
            }

            // Parse safety level
            if let Some(Value::Str(level)) = rec.get("safety_level") {
                config.max_safety_level = match level.to_lowercase().as_str() {
                    "safe" => SafetyLevel::Safe,
                    "caution" => SafetyLevel::Caution,
                    "dangerous" => SafetyLevel::Dangerous,
                    "critical" => SafetyLevel::Critical,
                    _ => SafetyLevel::Caution,
                };
            }
        }
    }

    let server = McpServer::with_config(config);
    let tools = server.list_tools();

    // Apply search filter
    let filtered_tools: Vec<_> = if let Some(ref search) = search_filter {
        let search_lower = search.to_lowercase();
        tools
            .into_iter()
            .filter(|t| {
                t.name.to_lowercase().contains(&search_lower)
                    || t.description.to_lowercase().contains(&search_lower)
            })
            .collect()
    } else {
        tools
    };

    let tool_records: Vec<Value> = filtered_tools
        .into_iter()
        .map(|tool| {
            let mut rec = BTreeMap::new();
            rec.insert("name".to_string(), Value::Str(tool.name));
            rec.insert("description".to_string(), Value::Str(tool.description));
            rec.insert(
                "input_schema".to_string(),
                Value::Str(tool.input_schema.to_string()),
            );
            Value::Record(rec)
        })
        .collect();

    Ok(Value::Array(tool_records))
}

/// mcp_call(name, args?) - Call an MCP tool by name with arguments
/// Args:
///   - name: String - Tool name to call
///   - args (optional): Record - Arguments for the tool
/// Returns: Record with result content or error
///
/// Example:
///   let result = mcp_call("ls", { path: "." })
///   let files = mcp_call("find", { directory: "/home", pattern: "*.rs" })
fn bi_mcp_call(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    use crate::mcp::{McpContent, McpServer, McpToolCall};
    use std::collections::HashMap;

    if args.is_empty() {
        return Err(anyhow!("mcp_call requires tool name as first argument"));
    }

    let tool_name = expect_string("mcp_call", &args[0])?;

    // Build arguments HashMap
    let tool_args: HashMap<String, serde_json::Value> = if args.len() > 1 {
        match &args[1] {
            Value::Record(rec) => rec
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json_owned(v.clone())))
                .collect(),
            _ => HashMap::new(),
        }
    } else {
        HashMap::new()
    };

    let server = McpServer::new();
    let call = McpToolCall {
        name: tool_name.to_string(),
        arguments: tool_args,
    };

    let result = server.call_tool(call);

    let mut record = BTreeMap::new();
    record.insert(
        "is_error".to_string(),
        Value::Bool(result.is_error.unwrap_or(false)),
    );

    let content_values: Vec<Value> = result
        .content
        .into_iter()
        .map(|c| {
            let mut content_rec = BTreeMap::new();
            match c {
                McpContent::Text { text } => {
                    content_rec.insert("type".to_string(), Value::Str("text".to_string()));
                    content_rec.insert("text".to_string(), Value::Str(text));
                }
                McpContent::Image { data, mime_type } => {
                    content_rec.insert("type".to_string(), Value::Str("image".to_string()));
                    content_rec.insert("data".to_string(), Value::Str(data));
                    content_rec.insert("mime_type".to_string(), Value::Str(mime_type));
                }
                McpContent::Resource { uri, mime_type } => {
                    content_rec.insert("type".to_string(), Value::Str("resource".to_string()));
                    content_rec.insert("uri".to_string(), Value::Str(uri));
                    if let Some(mt) = mime_type {
                        content_rec.insert("mime_type".to_string(), Value::Str(mt));
                    }
                }
            }
            Value::Record(content_rec)
        })
        .collect();
    record.insert("content".to_string(), Value::Array(content_values));

    Ok(Value::Record(record))
}

/// mcp_resources(type?) - List available MCP resources
/// Args:
///   - type (optional): String - Filter by resource type ("text", "blob", etc.)
/// Returns: Array of resource records
///
/// Example:
///   let resources = mcp_resources()
///   let text_resources = mcp_resources("text")
fn bi_mcp_resources(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    use crate::mcp::McpServer;

    let type_filter = if !args.is_empty() {
        if let Value::Str(s) = &args[0] {
            Some(s.clone())
        } else {
            None
        }
    } else {
        None
    };

    let server = McpServer::new();
    let resources = server.list_resources();

    let filtered: Vec<_> = if let Some(ref t) = type_filter {
        resources
            .into_iter()
            .filter(|r| r.mime_type.as_ref().map(|m| m.contains(t)).unwrap_or(false))
            .collect()
    } else {
        resources
    };

    let resource_records: Vec<Value> = filtered
        .into_iter()
        .map(|res| {
            let mut rec = BTreeMap::new();
            rec.insert("uri".to_string(), Value::Str(res.uri));
            rec.insert("name".to_string(), Value::Str(res.name));
            if let Some(desc) = res.description {
                rec.insert("description".to_string(), Value::Str(desc));
            }
            if let Some(mime) = res.mime_type {
                rec.insert("mime_type".to_string(), Value::Str(mime));
            }
            Value::Record(rec)
        })
        .collect();

    Ok(Value::Array(resource_records))
}

/// mcp_prompts(name?) - List available MCP prompts or get specific prompt
/// Args:
///   - name (optional): String - Get specific prompt by name
/// Returns: Array of prompt records or single prompt
///
/// Example:
///   let prompts = mcp_prompts()
///   let find_prompt = mcp_prompts("find-tool")
fn bi_mcp_prompts(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    use crate::mcp::McpServer;

    let server = McpServer::new();
    let prompts = server.list_prompts();

    if !args.is_empty() {
        if let Value::Str(name) = &args[0] {
            if let Some(prompt) = prompts.into_iter().find(|p| &p.name == name) {
                let mut rec = BTreeMap::new();
                rec.insert("name".to_string(), Value::Str(prompt.name));
                if let Some(desc) = prompt.description {
                    rec.insert("description".to_string(), Value::Str(desc));
                }
                if let Some(arguments) = prompt.arguments {
                    let args_values: Vec<Value> = arguments
                        .into_iter()
                        .map(|arg| {
                            let mut arg_rec = BTreeMap::new();
                            arg_rec.insert("name".to_string(), Value::Str(arg.name));
                            if let Some(desc) = arg.description {
                                arg_rec.insert("description".to_string(), Value::Str(desc));
                            }
                            arg_rec.insert(
                                "required".to_string(),
                                Value::Bool(arg.required.unwrap_or(false)),
                            );
                            Value::Record(arg_rec)
                        })
                        .collect();
                    rec.insert("arguments".to_string(), Value::Array(args_values));
                }
                return Ok(Value::Record(rec));
            }
            return Ok(Value::Null);
        }
    }

    let prompt_records: Vec<Value> = prompts
        .into_iter()
        .map(|prompt| {
            let mut rec = BTreeMap::new();
            rec.insert("name".to_string(), Value::Str(prompt.name));
            if let Some(desc) = prompt.description {
                rec.insert("description".to_string(), Value::Str(desc));
            }
            if let Some(arguments) = prompt.arguments {
                let args_values: Vec<Value> = arguments
                    .into_iter()
                    .map(|arg| {
                        let mut arg_rec = BTreeMap::new();
                        arg_rec.insert("name".to_string(), Value::Str(arg.name));
                        if let Some(desc) = arg.description {
                            arg_rec.insert("description".to_string(), Value::Str(desc));
                        }
                        arg_rec.insert(
                            "required".to_string(),
                            Value::Bool(arg.required.unwrap_or(false)),
                        );
                        Value::Record(arg_rec)
                    })
                    .collect();
                rec.insert("arguments".to_string(), Value::Array(args_values));
            }
            Value::Record(rec)
        })
        .collect();

    Ok(Value::Array(prompt_records))
}

/// mcp_connect(endpoint) - Connect to an external MCP server
/// Args:
///   - endpoint: String - Server endpoint URL (e.g., "http://localhost:3001")
/// Returns: Record with connection info and available tools
///
/// Example:
///   let client = mcp_connect("http://localhost:3001")
///   let tools = client.tools
fn bi_mcp_connect(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("mcp_connect requires endpoint URL"));
    }

    let endpoint = expect_string("mcp_connect", &args[0])?;

    // Try to detect MCP server at endpoint
    let servers = crate::ai::detect_mcp_servers();
    if let Some(server) = servers.iter().find(|s| s.endpoint == endpoint) {
        let mut record = BTreeMap::new();
        record.insert("endpoint".to_string(), Value::Str(server.endpoint.clone()));
        record.insert("name".to_string(), Value::Str(server.name.clone()));
        record.insert("available".to_string(), Value::Bool(server.available));

        let tools: Vec<Value> = server.tools.iter().map(|t| Value::Str(t.clone())).collect();
        record.insert("tools".to_string(), Value::Array(tools));

        return Ok(Value::Record(record));
    }

    // Server not detected, return basic info indicating unavailable
    let mut record = BTreeMap::new();
    record.insert("endpoint".to_string(), Value::Str(endpoint.to_string()));
    record.insert("name".to_string(), Value::Str("unknown".to_string()));
    record.insert("available".to_string(), Value::Bool(false));
    record.insert("tools".to_string(), Value::Array(vec![]));
    record.insert(
        "error".to_string(),
        Value::Str("Server not responding or not detected".to_string()),
    );

    Ok(Value::Record(record))
}

// Helper function to convert owned Value to serde_json::Value
fn value_to_json_owned(v: Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Int(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        Value::Float(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Str(s) => serde_json::Value::String(s),
        Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(value_to_json_owned).collect())
        }
        Value::Record(rec) => {
            let map: serde_json::Map<String, serde_json::Value> = rec
                .into_iter()
                .map(|(k, v)| (k, value_to_json_owned(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        _ => serde_json::Value::Null,
    }
}

/// first(n?) - Get first n elements from array input (default: 1)
/// Args:
///   - n (optional): Int - Number of elements to take (default: 1)
/// Returns: Array with first n elements, or Value if n=1
///
/// Example:
///   [1,2,3,4,5] | first()      # Returns 1
///   [1,2,3,4,5] | first(3)     # Returns [1,2,3]
fn bi_first(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr_val, count_arg) = if let Some(inp) = input {
        // Piped: input is array, args[0] is optional count
        (inp, args.get(0))
    } else if !args.is_empty() {
        // Not piped: args[0] is array, args[1] is optional count
        (args[0].clone(), args.get(1))
    } else {
        return Err(anyhow!("first requires array input"));
    };

    let arr = expect_array("first", &arr_val)?;

    let count = if let Some(c_arg) = count_arg {
        expect_int("first", c_arg)?
    } else {
        1
    };

    if count <= 0 {
        return Ok(Value::Array(vec![]));
    }

    // Fast path for single element (most common case)
    if count == 1 {
        return if arr.is_empty() {
            Ok(Value::Array(vec![]))
        } else {
            Ok(arr[0].clone())
        };
    }

    // Use iterator for all other cases (proven to be fastest)
    let result: Vec<Value> = arr.iter().take(count as usize).cloned().collect();
    Ok(Value::Array(result))
}

/// last(n?) - Get last n elements from array input (default: 1)
/// Args:
///   - n (optional): Int - Number of elements to take (default: 1)
/// Returns: Array with last n elements, or Value if n=1
///
/// Example:
///   [1,2,3,4,5] | last()       # Returns 5
///   [1,2,3,4,5] | last(3)      # Returns [3,4,5]
fn bi_last(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr_val, count_arg) = if let Some(inp) = input {
        // Piped: input is array, args[0] is optional count
        (inp, args.get(0))
    } else if !args.is_empty() {
        // Not piped: args[0] is array, args[1] is optional count
        (args[0].clone(), args.get(1))
    } else {
        return Err(anyhow!("last requires array input"));
    };

    let arr = expect_array("last", &arr_val)?;

    let count = if let Some(c_arg) = count_arg {
        expect_int("last", c_arg)?
    } else {
        1
    };

    if count <= 0 {
        return Ok(Value::Array(vec![]));
    }

    // Fast path for single element (most common case)
    if count == 1 {
        return if arr.is_empty() {
            Ok(Value::Array(vec![]))
        } else {
            Ok(arr[arr.len() - 1].clone())
        };
    }

    // Use slice for all other cases (proven to be fastest)
    let start_idx = if arr.len() > count as usize {
        arr.len() - count as usize
    } else {
        0
    };

    let result: Vec<Value> = arr[start_idx..].to_vec();
    Ok(Value::Array(result))
}

/// any(predicate?) - Check if any element matches predicate (or any truthy if no predicate)
/// Args:
///   - predicate (optional): Lambda - Function to test each element
/// Returns: Bool - true if any element matches
///
/// Example:
///   [1,2,3] | any(fn(x) => x > 2)     # Returns true
///   [false, true, false] | any()      # Returns true
fn bi_any(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let input_val = input.unwrap_or(Value::Array(vec![]));
    let arr = expect_array("any", &input_val)?;

    if args.is_empty() {
        // No predicate - check for any truthy values
        for val in arr {
            if is_truthy(val) {
                return Ok(Value::Bool(true));
            }
        }
        Ok(Value::Bool(false))
    } else {
        // Use predicate function
        let predicate = need_lambda(&args[0], "any")?;

        for val in arr {
            let result = call_lambda(predicate, &[val.clone()], env)?;
            if is_truthy(&result) {
                return Ok(Value::Bool(true));
            }
        }
        Ok(Value::Bool(false))
    }
}

/// all(predicate?) - Check if all elements match predicate (or all truthy if no predicate)
/// Args:
///   - predicate (optional): Lambda - Function to test each element
/// Returns: Bool - true if all elements match
///
/// Example:
///   [1,2,3] | all(fn(x) => x > 0)     # Returns true
///   [true, true, false] | all()       # Returns false
fn bi_all(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let input_val = input.unwrap_or(Value::Array(vec![]));
    let arr = expect_array("all", &input_val)?;

    if args.is_empty() {
        // No predicate - check if all values are truthy
        for val in arr {
            if !is_truthy(val) {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    } else {
        // Use predicate function
        let predicate = need_lambda(&args[0], "all")?;

        for val in arr {
            let result = call_lambda(predicate, &[val.clone()], env)?;
            if !is_truthy(&result) {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    }
}

/// Helper function to determine truthiness of a Value
fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Bool(b) => *b,
        Value::Int(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Record(r) => !r.is_empty(),
        Value::Null => false,
        Value::Lambda(_) => true,              // Functions are truthy
        Value::AsyncLambda(_) => true,         // Async lambdas are truthy
        Value::Future(_) => true,              // Futures are truthy
        Value::Uri(_) => true,                 // URIs are truthy if they exist
        Value::Table(t) => !t.rows.is_empty(), // Tables are truthy if they have data
    }
}

// ==================== String Functions ====================

/// split(delimiter) - Split string into array by delimiter
/// Args:
///   - delimiter: String - The delimiter to split on
/// Returns: Array - Array of string parts
///
/// Example:
///   "a,b,c" | split(",")     # Returns ["a", "b", "c"]
fn bi_split(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (text_val, delim_arg) = if let Some(inp) = input {
        // Piped: input is string, args[0] is delimiter
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("split requires delimiter argument"))?,
        )
    } else {
        // Not piped: args[0] is string, args[1] is delimiter
        (
            args.get(0)
                .ok_or_else(|| anyhow!("split requires string input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("split requires delimiter argument"))?,
        )
    };

    let text = expect_string("split", &text_val)?;
    let delimiter = expect_string("split", delim_arg)?;

    let parts: Vec<Value> = text
        .split(delimiter)
        .map(|s| Value::Str(s.to_string()))
        .collect();

    Ok(Value::Array(parts))
}

/// join(delimiter) - Join array elements into string with delimiter
/// Args:
///   - delimiter: String - The delimiter to join with
/// Returns: String - Joined string
///
/// Example:
///   ["a", "b", "c"] | join(",")     # Returns "a,b,c"
fn bi_join(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr_val, delim_arg) = if let Some(inp) = input {
        // Piped: input is array, args[0] is delimiter
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("join requires delimiter argument"))?,
        )
    } else {
        // Not piped: args[0] is array, args[1] is delimiter
        (
            args.get(0)
                .ok_or_else(|| anyhow!("join requires array input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("join requires delimiter argument"))?,
        )
    };

    let arr = expect_array("join", &arr_val)?;
    let delimiter = expect_string("join", delim_arg)?;

    let strings: Result<Vec<String>> = arr
        .iter()
        .map(|v| match v {
            Value::Str(s) => Ok(s.clone()),
            Value::Int(i) => Ok(i.to_string()),
            Value::Float(f) => Ok(f.to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            _ => Err(anyhow!("join requires array of strings or numbers")),
        })
        .collect();

    Ok(Value::Str(strings?.join(delimiter)))
}

/// trim() - Remove leading and trailing whitespace from string
/// Returns: String - Trimmed string
///
/// Example:
///   "  hello  " | trim()     # Returns "hello"
fn bi_trim(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("trim requires input string"))?;
    let text = expect_string("trim", &input_val)?;
    Ok(Value::Str(text.trim().to_string()))
}

/// upper() - Convert string to uppercase
/// Returns: String - Uppercase string
///
/// Example:
///   "hello" | upper()     # Returns "HELLO"
fn bi_upper(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("upper requires input string"))?;
    let text = expect_string("upper", &input_val)?;
    Ok(Value::Str(text.to_uppercase()))
}

/// lower() - Convert string to lowercase
/// Returns: String - Lowercase string
///
/// Example:
///   "HELLO" | lower()     # Returns "hello"
fn bi_lower(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("lower requires input string"))?;
    let text = expect_string("lower", &input_val)?;
    Ok(Value::Str(text.to_lowercase()))
}

/// replace(old, new) - Replace all occurrences of substring
/// Args:
///   - old: String - Substring to replace
///   - new: String - Replacement substring
/// Returns: String - String with replacements
///
/// Example:
///   "hello world" | replace("world", "universe")     # Returns "hello universe"
fn bi_replace(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (input_val, old_arg, new_arg) = if let Some(inp) = input {
        // Piped: input is string, args[0] is old, args[1] is new
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("replace requires old substring argument"))?,
            args.get(1)
                .ok_or_else(|| anyhow!("replace requires new substring argument"))?,
        )
    } else {
        // Not piped: args[0] is string, args[1] is old, args[2] is new
        (
            args.get(0)
                .ok_or_else(|| anyhow!("replace requires string input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("replace requires old substring argument"))?,
            args.get(2)
                .ok_or_else(|| anyhow!("replace requires new substring argument"))?,
        )
    };

    let text = expect_string("replace", &input_val)?;
    let old = expect_string("replace", old_arg)?;
    let new = expect_string("replace", new_arg)?;

    Ok(Value::Str(text.replace(old, new)))
}

/// contains(substring) - Check if string contains substring
/// Args:
///   - substring: String - Substring to search for
/// Returns: Bool - true if substring found
///
/// Example:
///   "hello world" | contains("world")     # Returns true
fn bi_contains(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (input_val, substring_arg) = if let Some(inp) = input {
        // Piped: input is string, args[0] is substring
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("contains requires substring argument"))?,
        )
    } else {
        // Not piped: args[0] is string, args[1] is substring
        (
            args.get(0)
                .ok_or_else(|| anyhow!("contains requires string input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("contains requires substring argument"))?,
        )
    };

    let text = expect_string("contains", &input_val)?;
    let substring = expect_string("contains", substring_arg)?;
    Ok(Value::Bool(text.contains(substring)))
}

/// starts_with(prefix) - Check if string starts with prefix
/// Args:
///   - prefix: String - Prefix to check
/// Returns: Bool - true if string starts with prefix
///
/// Example:
///   "hello world" | starts_with("hello")     # Returns true
fn bi_starts_with(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (input_val, prefix_arg) = if let Some(inp) = input {
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("starts_with requires prefix argument"))?,
        )
    } else {
        (
            args.get(0)
                .ok_or_else(|| anyhow!("starts_with requires string input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("starts_with requires prefix argument"))?,
        )
    };

    let text = expect_string("starts_with", &input_val)?;
    let prefix = expect_string("starts_with", prefix_arg)?;
    Ok(Value::Bool(text.starts_with(prefix)))
}

/// ends_with(suffix) - Check if string ends with suffix
/// Args:
///   - suffix: String - Suffix to check
/// Returns: Bool - true if string ends with suffix
///
/// Example:
///   "hello world" | ends_with("world")     # Returns true
fn bi_ends_with(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (input_val, suffix_arg) = if let Some(inp) = input {
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("ends_with requires suffix argument"))?,
        )
    } else {
        (
            args.get(0)
                .ok_or_else(|| anyhow!("ends_with requires string input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("ends_with requires suffix argument"))?,
        )
    };

    let text = expect_string("ends_with", &input_val)?;
    let suffix = expect_string("ends_with", suffix_arg)?;
    Ok(Value::Bool(text.ends_with(suffix)))
}

// ==================== Array Functions (Extended) ====================

/// flatten() - Flatten nested arrays one level deep
/// Returns: Array - Flattened array
///
/// Example:
///   [[1,2],[3,4],[5]] | flatten()     # Returns [1,2,3,4,5]
fn bi_flatten(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input.ok_or_else(|| anyhow!("flatten requires input array"))?;
    let arr = expect_array("flatten", &input_val)?;

    let mut result = Vec::new();
    for val in arr {
        match val {
            Value::Array(inner) => result.extend(inner.clone()),
            other => result.push(other.clone()),
        }
    }

    Ok(Value::Array(result))
}

/// reverse() - Reverse array or string
/// Returns: Array or String - Reversed input
///
/// Example:
///   [1,2,3,4,5] | reverse()     # Returns [5,4,3,2,1]
fn bi_reverse(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("reverse requires input"))?;

    match input_val {
        Value::Array(mut arr) => {
            arr.reverse();
            Ok(Value::Array(arr))
        }
        Value::Str(s) => Ok(Value::Str(s.chars().rev().collect())),
        _ => Err(anyhow!("reverse requires array or string input")),
    }
}

/// slice(start, end?) - Extract slice of array or string
/// Args:
///   - start: Int - Start index (inclusive)
///   - end: Int (optional) - End index (exclusive)
/// Returns: Array or String - Sliced input
///
/// Example:
///   [1,2,3,4,5] | slice(1, 3)     # Returns [2,3]
fn bi_slice(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input.ok_or_else(|| anyhow!("slice requires input"))?;

    if args.is_empty() {
        return Err(anyhow!("slice requires at least start index"));
    }

    let start = expect_int("slice", &args[0])? as usize;

    match input_val {
        Value::Array(arr) => {
            let end = if args.len() > 1 {
                expect_int("slice", &args[1])? as usize
            } else {
                arr.len()
            };

            let end = end.min(arr.len());
            let start = start.min(end);

            Ok(Value::Array(arr[start..end].to_vec()))
        }
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let end = if args.len() > 1 {
                expect_int("slice", &args[1])? as usize
            } else {
                chars.len()
            };

            let end = end.min(chars.len());
            let start = start.min(end);

            Ok(Value::Str(chars[start..end].iter().collect()))
        }
        _ => Err(anyhow!("slice requires array or string input")),
    }
}

/// range(start, end?, step?) - Generate array of integers
/// Args:
///   - start: Int - Start value (or end if only one arg)
///   - end: Int (optional) - End value (exclusive)
///   - step: Int (optional) - Step size (default 1)
/// Returns: Array - Array of integers
///
/// Example:
///   range(5)           # Returns [0,1,2,3,4]
///   range(2, 7)        # Returns [2,3,4,5,6]
///   range(0, 10, 2)    # Returns [0,2,4,6,8]
fn bi_range(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("range requires at least one argument"));
    }

    let (start, end, step) = if args.len() == 1 {
        // range(n) -> 0..n
        (0, expect_int("range", &args[0])?, 1)
    } else if args.len() == 2 {
        // range(start, end) -> start..end
        (
            expect_int("range", &args[0])?,
            expect_int("range", &args[1])?,
            1,
        )
    } else {
        // range(start, end, step)
        (
            expect_int("range", &args[0])?,
            expect_int("range", &args[1])?,
            expect_int("range", &args[2])?,
        )
    };

    if step == 0 {
        return Err(anyhow!("range step cannot be zero"));
    }

    let mut result = Vec::new();
    let mut current = start;

    if step > 0 {
        while current < end {
            result.push(Value::Int(current));
            current += step;
        }
    } else {
        while current > end {
            result.push(Value::Int(current));
            current += step;
        }
    }

    Ok(Value::Array(result))
}

/// zip(array2) - Zip two arrays into array of pairs
/// Args:
///   - array2: Array - Second array to zip with
/// Returns: Array - Array of [item1, item2] pairs
///
/// Example:
///   [1,2,3] | zip(["a","b","c"])     # Returns [[1,"a"],[2,"b"],[3,"c"]]
fn bi_zip(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr1_val, arr2_val) = if let Some(inp) = input {
        // Piped: input is first array, args[0] is second array
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("zip requires second array argument"))?
                .clone(),
        )
    } else {
        // Not piped: args[0] is first array, args[1] is second array
        (
            args.get(0)
                .ok_or_else(|| anyhow!("zip requires two array arguments"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("zip requires two array arguments"))?
                .clone(),
        )
    };

    let arr1 = expect_array("zip", &arr1_val)?;
    let arr2 = expect_array("zip", &arr2_val)?;
    let mut result = Vec::new();

    for (a, b) in arr1.iter().zip(arr2.iter()) {
        result.push(Value::Array(vec![a.clone(), b.clone()]));
    }

    Ok(Value::Array(result))
}

/// push(item) - Add item to end of array
/// Args:
///   - item: Any - Item to add
/// Returns: Array - Array with item added
///
/// Example:
///   [1,2,3] | push(4)     # Returns [1,2,3,4]
fn bi_push(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr_val, item) = if let Some(inp) = input {
        // Piped: input is array, args[0] is item
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("push requires item argument"))?,
        )
    } else {
        // Not piped: args[0] is array, args[1] is item
        (
            args.get(0)
                .ok_or_else(|| anyhow!("push requires array input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("push requires item argument"))?,
        )
    };

    let mut arr = expect_array("push", &arr_val)?.to_vec();
    arr.push(item.clone());
    Ok(Value::Array(arr))
}

/// concat(array2) - Concatenate two arrays
/// Args:
///   - array2: Array - Array to concatenate
/// Returns: Array - Combined array
///
/// Example:
///   [1,2,3] | concat([4,5,6])     # Returns [1,2,3,4,5,6]
fn bi_concat(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr1_val, arr2_val) = if let Some(inp) = input {
        // Piped: input is first array, args[0] is second array
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("concat requires second array argument"))?
                .clone(),
        )
    } else {
        // Not piped: args[0] is first array, args[1] is second array
        (
            args.get(0)
                .ok_or_else(|| anyhow!("concat requires two array arguments"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("concat requires two array arguments"))?
                .clone(),
        )
    };

    let mut arr1 = expect_array("concat", &arr1_val)?.to_vec();
    let arr2 = expect_array("concat", &arr2_val)?;
    arr1.extend(arr2.iter().cloned());

    Ok(Value::Array(arr1))
}

// ==================== Math Functions ====================

/// abs(number?) - Get absolute value
/// Args:
///   - number: Number (optional if piped)
/// Returns: Number - Absolute value
///
/// Example:
///   abs(-5)     # Returns 5
fn bi_abs(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("abs requires number argument"))?
    } else {
        args[0].clone()
    };

    match val {
        Value::Int(n) => Ok(Value::Int(n.abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        _ => Err(anyhow!("abs requires numeric input")),
    }
}

/// min(a, b) - Get minimum of two numbers
/// Args:
///   - a: Number
///   - b: Number
/// Returns: Number - Minimum value
///
/// Example:
///   min(3, 7)     # Returns 3
fn bi_min(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("min requires two arguments"));
    }

    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.min(b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.min(*b))),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).min(*b))),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.min(*b as f64))),
        _ => Err(anyhow!("min requires numeric arguments")),
    }
}

/// max(a, b) - Get maximum of two numbers
/// Args:
///   - a: Number
///   - b: Number
/// Returns: Number - Maximum value
///
/// Example:
///   max(3, 7)     # Returns 7
fn bi_max(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("max requires two arguments"));
    }

    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.max(b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.max(*b))),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).max(*b))),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.max(*b as f64))),
        _ => Err(anyhow!("max requires numeric arguments")),
    }
}

/// floor(number) - Round down to nearest integer
/// Args:
///   - number: Number
/// Returns: Int - Rounded down value
///
/// Example:
///   floor(3.7)     # Returns 3
fn bi_floor(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("floor requires number argument"))?
    } else {
        args[0].clone()
    };

    match val {
        Value::Int(n) => Ok(Value::Int(n)),
        Value::Float(f) => Ok(Value::Int(f.floor() as i64)),
        _ => Err(anyhow!("floor requires numeric input")),
    }
}

/// ceil(number) - Round up to nearest integer
/// Args:
///   - number: Number
/// Returns: Int - Rounded up value
///
/// Example:
///   ceil(3.2)     # Returns 4
fn bi_ceil(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("ceil requires number argument"))?
    } else {
        args[0].clone()
    };

    match val {
        Value::Int(n) => Ok(Value::Int(n)),
        Value::Float(f) => Ok(Value::Int(f.ceil() as i64)),
        _ => Err(anyhow!("ceil requires numeric input")),
    }
}

/// round(number) - Round to nearest integer
/// Args:
///   - number: Number
/// Returns: Int - Rounded value
///
/// Example:
///   round(3.5)     # Returns 4
fn bi_round(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("round requires number argument"))?
    } else {
        args[0].clone()
    };

    match val {
        Value::Int(n) => Ok(Value::Int(n)),
        Value::Float(f) => Ok(Value::Int(f.round() as i64)),
        _ => Err(anyhow!("round requires numeric input")),
    }
}

/// sqrt(number) - Calculate square root
/// Args:
///   - number: Number
/// Returns: Float - Square root
///
/// Example:
///   sqrt(16)     # Returns 4.0
fn bi_sqrt(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("sqrt requires number argument"))?
    } else {
        args[0].clone()
    };

    let num = match val {
        Value::Int(n) => n as f64,
        Value::Float(f) => f,
        _ => return Err(anyhow!("sqrt requires numeric input")),
    };

    if num < 0.0 {
        return Err(anyhow!("sqrt requires non-negative number"));
    }

    Ok(Value::Float(num.sqrt()))
}

/// pow(base, exponent) - Calculate power
/// Args:
///   - base: Number
///   - exponent: Number
/// Returns: Number - base ^ exponent
///
/// Example:
///   pow(2, 8)     # Returns 256
fn bi_pow(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("pow requires two arguments: base and exponent"));
    }

    let base = match &args[0] {
        Value::Int(n) => *n as f64,
        Value::Float(f) => *f,
        _ => return Err(anyhow!("pow requires numeric base")),
    };

    let exp = match &args[1] {
        Value::Int(n) => *n as f64,
        Value::Float(f) => *f,
        _ => return Err(anyhow!("pow requires numeric exponent")),
    };

    let result = base.powf(exp);

    // Return Int if result is a whole number and both inputs were Int
    if matches!((&args[0], &args[1]), (Value::Int(_), Value::Int(_))) && result.fract() == 0.0 {
        Ok(Value::Int(result as i64))
    } else {
        Ok(Value::Float(result))
    }
}

// ==================== Utility Functions ====================

/// exit(code?) - Exit the program
/// Args:
///   - code: Int (optional) - Exit code (default 0)
/// Returns: Never returns
///
/// Example:
///   exit(0)     # Exit with success
fn bi_exit(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let code = if args.is_empty() {
        0
    } else {
        expect_int("exit", &args[0])?
    };

    std::process::exit(code as i32);
}

/// env(key) - Get environment variable
/// Args:
///   - key: String - Environment variable name
/// Returns: String or Null - Variable value or null if not set
///
/// Example:
///   env("HOME")     # Returns home directory path
fn bi_env(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("env requires variable name argument"));
    }

    let key = expect_string("env", &args[0])?;
    match std::env::var(key) {
        Ok(val) => Ok(Value::Str(val)),
        Err(_) => Ok(Value::Null),
    }
}

/// set_env(key, value) - Set environment variable
/// Args:
///   - key: String - Environment variable name
///   - value: String - Value to set
/// Returns: Null
///
/// Example:
///   set_env("MY_VAR", "value")
fn bi_set_env(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("set_env requires key and value arguments"));
    }

    let key = expect_string("set_env", &args[0])?;
    let value = expect_string("set_env", &args[1])?;

    std::env::set_var(key, value);
    Ok(Value::Null)
}

/// sleep(seconds) - Sleep for specified duration
/// Args:
///   - seconds: Number - Duration to sleep
/// Returns: Null
///
/// Example:
///   sleep(2)     # Sleep for 2 seconds
fn bi_sleep(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("sleep requires duration argument"));
    }

    let seconds = match &args[0] {
        Value::Int(n) => *n as f64,
        Value::Float(f) => *f,
        _ => return Err(anyhow!("sleep requires numeric duration")),
    };

    if seconds < 0.0 {
        return Err(anyhow!("sleep duration must be non-negative"));
    }

    std::thread::sleep(Duration::from_secs_f64(seconds));
    Ok(Value::Null)
}

/// time() - Get current Unix timestamp
/// Returns: Int - Current Unix timestamp in seconds
///
/// Example:
///   time()     # Returns 1699401234
fn bi_time(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| anyhow!("Failed to get system time: {}", e))?
        .as_secs();

    Ok(Value::Int(timestamp as i64))
}

/// json_parse(json_string?) - Parse JSON string into value
/// Args:
///   - json_string: String (optional if piped)
/// Returns: Any - Parsed JSON value
///
/// Example:
///   json_parse("{\"a\":1}")     # Returns {a: 1}
fn bi_json_parse(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let json_str = if args.is_empty() {
        let input_val = input.ok_or_else(|| anyhow!("json_parse requires JSON string"))?;
        expect_string("json_parse", &input_val)?.to_string()
    } else {
        expect_string("json_parse", &args[0])?.to_string()
    };

    let json_val: serde_json::Value =
        serde_json::from_str(&json_str).context("Failed to parse JSON")?;

    Ok(json_to_value(json_val))
}

/// json_stringify(value?) - Convert value to JSON string
/// Args:
///   - value: Any (optional if piped)
/// Returns: String - JSON representation
///
/// Example:
///   {a: 1} | json_stringify()     # Returns "{\"a\":1}"
fn bi_json_stringify(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("json_stringify requires value"))?
    } else {
        args[0].clone()
    };

    let json_val = value_to_json(val);
    let json_str = serde_json::to_string(&json_val).context("Failed to stringify to JSON")?;

    Ok(Value::Str(json_str))
}

// ==================== Syntax Knowledge Base Functions ====================

use crate::syntax_kb::{AgenticBinary, SyntaxCategory, SyntaxEntry, SyntaxKB};
use std::sync::{Mutex, OnceLock};

// Global syntax knowledge base
static SYNTAX_KB: OnceLock<Mutex<SyntaxKB>> = OnceLock::new();

fn get_syntax_kb() -> &'static Mutex<SyntaxKB> {
    SYNTAX_KB.get_or_init(|| {
        // Try to load from ~/.aethershell/syntax_kb.json
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        let storage_path = std::path::PathBuf::from(home)
            .join(".aethershell")
            .join("syntax_kb.json");

        // Create directory if it doesn't exist
        if let Some(parent) = storage_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let kb = SyntaxKB::with_storage(storage_path).unwrap_or_else(|_| SyntaxKB::new());
        Mutex::new(kb)
    })
}

/// syntax_get(id) - Get syntax entry by ID
/// Args:
///   - id: String - Syntax ID
/// Returns: Record - Syntax entry details
///
/// Example:
///   syntax_get("ab")  # Returns AgenticBinary protocol details
fn bi_syntax_get(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let id_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("syntax_get requires syntax ID"))?;
    let id = expect_string("syntax_get", &id_val)?;

    let kb = get_syntax_kb().lock().unwrap();
    let entry = kb
        .get(id)
        .ok_or_else(|| anyhow!("Syntax '{}' not found", id))?;

    // Convert to Value::Record
    let mut fields = std::collections::BTreeMap::new();
    fields.insert("id".to_string(), Value::Str(entry.id.clone()));
    fields.insert("name".to_string(), Value::Str(entry.name.clone()));
    fields.insert(
        "category".to_string(),
        Value::Str(format!("{:?}", entry.category)),
    );
    fields.insert(
        "specification".to_string(),
        Value::Str(entry.specification.clone()),
    );
    fields.insert(
        "examples".to_string(),
        Value::Array(
            entry
                .examples
                .iter()
                .map(|s| Value::Str(s.clone()))
                .collect(),
        ),
    );

    if let Some(enc) = &entry.binary_encoding {
        let mut enc_fields = std::collections::BTreeMap::new();
        enc_fields.insert("name".to_string(), Value::Str(enc.name.clone()));
        enc_fields.insert("bit_layout".to_string(), Value::Str(enc.bit_layout.clone()));
        fields.insert("binary_encoding".to_string(), Value::Record(enc_fields));
    }

    Ok(Value::Record(fields))
}

/// syntax_list(category?) - List all syntax entries or by category
/// Args:
///   - category: String (optional) - Category to filter by
/// Returns: Array - List of syntax IDs
///
/// Example:
///   syntax_list()           # Returns all syntax IDs
///   syntax_list("protocol") # Returns protocol syntax IDs
fn bi_syntax_list(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let kb = get_syntax_kb().lock().unwrap();

    let ids = if let Some(cat_val) = input.or_else(|| args.get(0).cloned()) {
        let category_str = expect_string("syntax_list", &cat_val)?;
        let category = match category_str.to_lowercase().as_str() {
            "protocol" => SyntaxCategory::Protocol,
            "language" => SyntaxCategory::Language,
            "encoding" => SyntaxCategory::Encoding,
            "command" => SyntaxCategory::Command,
            "query" => SyntaxCategory::Query,
            other => SyntaxCategory::Custom(other.to_string()),
        };

        kb.list_by_category(&category)
            .iter()
            .map(|e| Value::Str(e.id.clone()))
            .collect()
    } else {
        kb.list_all_ids()
            .iter()
            .map(|id| Value::Str(id.clone()))
            .collect()
    };

    Ok(Value::Array(ids))
}

/// syntax_search(query) - Search syntax entries by keyword
/// Args:
///   - query: String - Search query
/// Returns: Array - Matching syntax IDs
///
/// Example:
///   syntax_search("binary")  # Finds entries with "binary" in name/spec
fn bi_syntax_search(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let query_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("syntax_search requires query string"))?;
    let query = expect_string("syntax_search", &query_val)?;

    let kb = get_syntax_kb().lock().unwrap();
    let results: Vec<Value> = kb
        .search(query)
        .iter()
        .map(|e| Value::Str(e.id.clone()))
        .collect();

    Ok(Value::Array(results))
}

/// syntax_add(entry_record) - Add a new syntax entry
/// Args:
///   - entry_record: Record - Syntax entry with id, name, category, specification
/// Returns: Bool - Success status
///
/// Example:
///   syntax_add({id: "mysyntax", name: "My Syntax", category: "custom", specification: "..."})
fn bi_syntax_add(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let entry_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("syntax_add requires entry record"))?;

    let record = match entry_val {
        Value::Record(r) => r,
        _ => return Err(anyhow!("syntax_add requires Record type")),
    };

    // Extract required fields
    let id = match record.get("id") {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(anyhow!("syntax_add requires 'id' field")),
    };

    let name = match record.get("name") {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(anyhow!("syntax_add requires 'name' field")),
    };

    let category_str = match record.get("category") {
        Some(Value::Str(s)) => s.clone(),
        _ => "custom".to_string(),
    };

    let category = match category_str.to_lowercase().as_str() {
        "protocol" => SyntaxCategory::Protocol,
        "language" => SyntaxCategory::Language,
        "encoding" => SyntaxCategory::Encoding,
        "command" => SyntaxCategory::Command,
        "query" => SyntaxCategory::Query,
        other => SyntaxCategory::Custom(other.to_string()),
    };

    let specification = match record.get("specification") {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(anyhow!("syntax_add requires 'specification' field")),
    };

    let examples = match record.get("examples") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| match v {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };

    let entry = SyntaxEntry {
        id,
        name,
        category,
        specification,
        examples,
        binary_encoding: None,
        metadata: std::collections::HashMap::new(),
    };

    let mut kb = get_syntax_kb().lock().unwrap();
    kb.add_entry(entry)?;

    Ok(Value::Bool(true))
}

/// syntax_categories() - List all available syntax categories
/// Returns: Array - List of category names
///
/// Example:
///   syntax_categories()  # Returns ["protocol", "language", ...]
fn bi_syntax_categories(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let kb = get_syntax_kb().lock().unwrap();
    let categories: Vec<Value> = kb
        .list_categories()
        .iter()
        .map(|c| Value::Str(c.clone()))
        .collect();

    Ok(Value::Array(categories))
}

/// ab_encode(msg_type, opcode, payload) - Encode message using AgenticBinary protocol
/// Args:
///   - msg_type: String or Int - Message type (Command/Query/Response/Event or 0-3)
///   - opcode: String or Int - Opcode name or number (0-15)
///   - payload: String - Payload data
/// Returns: Array - Encoded bytes
///
/// Example:
///   ab_encode("Query", "DATA", "hello")  # Encodes AgenticBinary message
///   ab_encode(1, 4, "hello")             # Same using numeric codes
fn bi_ab_encode(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 3 {
        return Err(anyhow!(
            "ab_encode requires 3 arguments: msg_type, opcode, payload"
        ));
    }

    // Parse message type
    let msg_type = match &args[0] {
        Value::Int(i) => *i as u8,
        Value::Str(s) => match s.to_lowercase().as_str() {
            "command" | "cmd" => 0,
            "query" => 1,
            "response" | "resp" => 2,
            "event" => 3,
            _ => return Err(anyhow!("Invalid message type: {}", s)),
        },
        _ => return Err(anyhow!("msg_type must be String or Int")),
    };

    // Parse opcode
    let opcode = match &args[1] {
        Value::Int(i) => *i as u8,
        Value::Str(s) => match s.to_uppercase().as_str() {
            "PING" => 0x0,
            "ACK" => 0x1,
            "QUERY" => 0x2,
            "EXEC" => 0x3,
            "DATA" => 0x4,
            "ERROR" => 0x5,
            "SYNC" => 0x6,
            "AUTH" => 0x7,
            "DELEGATE" => 0x8,
            "COLLABORATE" => 0x9,
            "LEARN" => 0xA,
            "REASON" => 0xB,
            "PLAN" => 0xC,
            "OBSERVE" => 0xD,
            "REFLECT" => 0xE,
            "EXTEND" => 0xF,
            _ => return Err(anyhow!("Invalid opcode: {}", s)),
        },
        _ => return Err(anyhow!("opcode must be String or Int")),
    };

    // Get payload
    let payload = expect_string("ab_encode", &args[2])?;

    // Encode using AgenticBinary (version 0)
    let encoded = AgenticBinary::encode(0, msg_type, opcode, payload.as_bytes())?;

    // Convert to Value::Array of Int values
    let result: Vec<Value> = encoded.iter().map(|&b| Value::Int(b as i64)).collect();

    Ok(Value::Array(result))
}

/// ab_decode(bytes) - Decode AgenticBinary message
/// Args:
///   - bytes: Array - Byte array to decode
/// Returns: Record - Decoded message with version, msg_type, opcode, payload
///
/// Example:
///   ab_decode([20, 5, 104, 101, 108, 108, 111])  # Decodes to record
fn bi_ab_decode(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let bytes_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("ab_decode requires byte array"))?;

    let bytes_arr = expect_array("ab_decode", &bytes_val)?;
    let bytes: Vec<u8> = bytes_arr
        .iter()
        .map(|v| match v {
            Value::Int(i) => Ok(*i as u8),
            _ => Err(anyhow!("Byte array must contain integers")),
        })
        .collect::<Result<Vec<u8>>>()?;

    let (version, msg_type, opcode, payload) = AgenticBinary::decode(&bytes)?;

    let mut result = std::collections::BTreeMap::new();
    result.insert("version".to_string(), Value::Int(version as i64));
    result.insert(
        "msg_type".to_string(),
        Value::Str(AgenticBinary::msg_type_name(msg_type).to_string()),
    );
    result.insert("msg_type_code".to_string(), Value::Int(msg_type as i64));
    result.insert(
        "opcode".to_string(),
        Value::Str(AgenticBinary::opcode_name(opcode).to_string()),
    );
    result.insert("opcode_code".to_string(), Value::Int(opcode as i64));
    result.insert(
        "payload".to_string(),
        Value::Str(String::from_utf8_lossy(&payload).to_string()),
    );
    result.insert(
        "payload_bytes".to_string(),
        Value::Array(payload.iter().map(|&b| Value::Int(b as i64)).collect()),
    );

    Ok(Value::Record(result))
}

// --------------- Neural Network Builtins ---------------

/// nn_create(name, layer_sizes, [hidden_activation], [output_activation])
/// Creates a feedforward neural network
/// Args:
///   - name: String - Network name/identifier
///   - layer_sizes: Array[Int] - Sizes of each layer [input, hidden..., output]
///   - hidden_activation: String (optional) - "relu", "sigmoid", "tanh", "swish" (default: "relu")
///   - output_activation: String (optional) - activation for output layer (default: "tanh")
/// Returns: Record - Network representation
///
/// Example:
///   nn_create("policy", [8, 16, 8, 4])  # 8 inputs, 2 hidden layers, 4 outputs
fn bi_nn_create(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("nn_create requires at least name and layer_sizes"));
    }

    let name = expect_string("nn_create", &args[0])?;
    let layer_sizes_val = args
        .get(1)
        .ok_or_else(|| anyhow!("nn_create requires layer_sizes array"))?;
    let layer_sizes_arr = expect_array("nn_create", layer_sizes_val)?;

    let layer_sizes: Vec<usize> = layer_sizes_arr
        .iter()
        .map(|v| expect_int("nn_create", v).map(|i| i as usize))
        .collect::<Result<Vec<_>>>()?;

    if layer_sizes.len() < 2 {
        return Err(anyhow!(
            "nn_create requires at least 2 layers (input and output)"
        ));
    }

    let hidden_activation = args
        .get(2)
        .and_then(|v| {
            if let Value::Str(s) = v {
                Some(s.as_str())
            } else {
                None
            }
        })
        .and_then(Activation::from_str)
        .unwrap_or(Activation::ReLU);

    let output_activation = args
        .get(3)
        .and_then(|v| {
            if let Value::Str(s) = v {
                Some(s.as_str())
            } else {
                None
            }
        })
        .and_then(Activation::from_str)
        .unwrap_or(Activation::Tanh);

    let network =
        NeuralNetwork::feedforward(name, &layer_sizes, hidden_activation, output_activation);
    nn_to_value(&network)
}

/// nn_forward(network, input)
/// Forward pass through the network
/// Args:
///   - network: Record - Network from nn_create
///   - input: Array[Float] - Input values
/// Returns: Array[Float] - Output values
fn bi_nn_forward(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let has_input = input.is_some();
    let network_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("nn_forward requires network"))?;

    let network = value_to_nn(&network_val)?;

    let input_val = args
        .get(if has_input { 0 } else { 1 })
        .ok_or_else(|| anyhow!("nn_forward requires input array"))?;
    let input_arr = expect_array("nn_forward", input_val)?;

    let input_vec: Vec<f64> = input_arr
        .iter()
        .map(|v| value_to_f64(v))
        .collect::<Result<Vec<_>>>()?;

    let output = network.forward(&input_vec);
    Ok(Value::Array(output.into_iter().map(Value::Float).collect()))
}

/// nn_mutate(network, rate, strength)
/// Mutate network parameters
/// Args:
///   - network: Record - Network to mutate
///   - rate: Float - Mutation probability (0.0-1.0)
///   - strength: Float - Mutation magnitude
/// Returns: Record - Mutated network
fn bi_nn_mutate(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let has_input = input.is_some();
    let network_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("nn_mutate requires network"))?;

    let network = value_to_nn(&network_val)?;

    let idx = if has_input { 0 } else { 1 };
    let rate = args.get(idx).map(value_to_f64).transpose()?.unwrap_or(0.1);
    let strength = args
        .get(idx + 1)
        .map(value_to_f64)
        .transpose()?
        .unwrap_or(0.3);

    let mutated = network.mutate(rate, strength);
    nn_to_value(&mutated)
}

/// nn_crossover(network1, network2)
/// Crossover two networks
/// Args:
///   - network1: Record - First parent network
///   - network2: Record - Second parent network
/// Returns: Record - Child network
fn bi_nn_crossover(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("nn_crossover requires two networks"));
    }

    let net1 = value_to_nn(&args[0])?;
    let net2 = value_to_nn(&args[1])?;

    let child = NeuralNetwork::crossover(&net1, &net2);
    nn_to_value(&child)
}

/// nn_params(network)
/// Get network parameters as flat array
/// Args:
///   - network: Record - Network
/// Returns: Array[Float] - All weights and biases
fn bi_nn_params(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let network_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("nn_params requires network"))?;

    let network = value_to_nn(&network_val)?;
    let params = network.get_params();
    Ok(Value::Array(params.into_iter().map(Value::Float).collect()))
}

/// nn_set_params(network, params)
/// Set network parameters from flat array
/// Args:
///   - network: Record - Network
///   - params: Array[Float] - New parameters
/// Returns: Record - Updated network
fn bi_nn_set_params(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let has_input = input.is_some();
    let network_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("nn_set_params requires network"))?;

    let mut network = value_to_nn(&network_val)?;

    let idx = if has_input { 0 } else { 1 };
    let params_val = args
        .get(idx)
        .ok_or_else(|| anyhow!("nn_set_params requires params array"))?;
    let params_arr = expect_array("nn_set_params", params_val)?;

    let params: Vec<f64> = params_arr
        .iter()
        .map(value_to_f64)
        .collect::<Result<Vec<_>>>()?;

    network.set_params(&params);
    nn_to_value(&network)
}

/// nn_layers(network)
/// Get layer information
/// Args:
///   - network: Record - Network
/// Returns: Array[Record] - Layer specifications
fn bi_nn_layers(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let network_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("nn_layers requires network"))?;

    let network = value_to_nn(&network_val)?;

    let layers: Vec<Value> = network
        .layers
        .iter()
        .map(|layer| {
            let mut rec = BTreeMap::new();
            rec.insert(
                "input_size".to_string(),
                Value::Int(layer.input_size as i64),
            );
            rec.insert(
                "output_size".to_string(),
                Value::Int(layer.output_size as i64),
            );
            rec.insert(
                "activation".to_string(),
                Value::Str(format!("{:?}", layer.activation)),
            );
            rec.insert(
                "param_count".to_string(),
                Value::Int(layer.param_count() as i64),
            );
            Value::Record(rec)
        })
        .collect();

    Ok(Value::Array(layers))
}

/// nn_info(network)
/// Get network summary information
/// Args:
///   - network: Record - Network
/// Returns: Record - Network info including total params, layer count, etc.
fn bi_nn_info(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let network_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("nn_info requires network"))?;

    let network = value_to_nn(&network_val)?;

    let mut info = BTreeMap::new();
    info.insert("name".to_string(), Value::Str(network.name.clone()));
    info.insert(
        "layer_count".to_string(),
        Value::Int(network.layers.len() as i64),
    );
    info.insert(
        "param_count".to_string(),
        Value::Int(network.param_count() as i64),
    );

    if let Some(first) = network.layers.first() {
        info.insert(
            "input_size".to_string(),
            Value::Int(first.input_size as i64),
        );
    }
    if let Some(last) = network.layers.last() {
        info.insert(
            "output_size".to_string(),
            Value::Int(last.output_size as i64),
        );
    }

    Ok(Value::Record(info))
}

/// consensus_net(name, agent_count, input_size, hidden_size, output_size)
/// Create a consensus network for distributed decision making
/// Args:
///   - name: String - Network name
///   - agent_count: Int - Number of agents
///   - input_size: Int - Input dimension
///   - hidden_size: Int - Hidden layer size
///   - output_size: Int - Output dimension (decision size)
/// Returns: Record - Consensus network
fn bi_consensus_net(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 5 {
        return Err(anyhow!(
            "consensus_net requires name, agent_count, input_size, hidden_size, output_size"
        ));
    }

    let name = expect_string("consensus_net", &args[0])?;
    let agent_count = expect_int("consensus_net", &args[1])? as usize;
    let input_size = expect_int("consensus_net", &args[2])? as usize;
    let hidden_size = expect_int("consensus_net", &args[3])? as usize;
    let output_size = expect_int("consensus_net", &args[4])? as usize;

    let consensus = ConsensusNetwork::new(name, agent_count, input_size, hidden_size, output_size);
    consensus_net_to_value(&consensus)
}

/// consensus_vote(network, agent_inputs)
/// Run consensus voting across all agents
/// Args:
///   - network: Record - Consensus network
///   - agent_inputs: Array[Array[Float]] - Each agent's input
/// Returns: Record - {decisions: Array[Array], consensus: Array[Float], confidence: Float}
fn bi_consensus_vote(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let has_input = input.is_some();
    let network_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("consensus_vote requires network"))?;

    let mut network = value_to_consensus_net(&network_val)?;

    let idx = if has_input { 0 } else { 1 };
    let inputs_val = args
        .get(idx)
        .ok_or_else(|| anyhow!("consensus_vote requires agent_inputs"))?;
    let inputs_arr = expect_array("consensus_vote", inputs_val)?;

    let agent_inputs: Vec<Vec<f64>> = inputs_arr
        .iter()
        .map(|v| {
            let arr = expect_array("consensus_vote", v)?;
            arr.iter().map(value_to_f64).collect()
        })
        .collect::<Result<Vec<_>>>()?;

    let (decisions, consensus) = network.consensus(&agent_inputs);

    // Calculate confidence as 1 - variance of decisions
    let decision_dim = decisions.first().map(|d| d.len()).unwrap_or(0);
    let mean: Vec<f64> = (0..decision_dim)
        .map(|i| {
            decisions
                .iter()
                .map(|d| d.get(i).copied().unwrap_or(0.0))
                .sum::<f64>()
                / decisions.len().max(1) as f64
        })
        .collect();
    let variance: f64 = decisions
        .iter()
        .map(|d: &Vec<f64>| {
            d.iter()
                .zip(&mean)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
        })
        .sum::<f64>()
        / decisions.len().max(1) as f64;
    let confidence = 1.0 / (1.0 + variance);

    let mut result = BTreeMap::new();
    result.insert(
        "decisions".to_string(),
        Value::Array(
            decisions
                .into_iter()
                .map(|d: Vec<f64>| Value::Array(d.into_iter().map(Value::Float).collect()))
                .collect(),
        ),
    );
    result.insert(
        "consensus".to_string(),
        Value::Array(consensus.into_iter().map(Value::Float).collect()),
    );
    result.insert("confidence".to_string(), Value::Float(confidence));

    Ok(Value::Record(result))
}

/// activation(name)
/// Get activation function by name
/// Args:
///   - name: String - "relu", "sigmoid", "tanh", "softmax", "linear", "swish", "leaky_relu"
/// Returns: Record - Activation function info
fn bi_activation(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let name = args
        .get(0)
        .and_then(|v| {
            if let Value::Str(s) = v {
                Some(s.as_str())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("activation requires name string"))?;

    let activation = Activation::from_str(name)
        .ok_or_else(|| anyhow!("Unknown activation: {}. Valid: relu, sigmoid, tanh, softmax, linear, swish, leaky_relu", name))?;

    let mut info = BTreeMap::new();
    info.insert("name".to_string(), Value::Str(format!("{:?}", activation)));
    info.insert(
        "description".to_string(),
        Value::Str(
            match activation {
                Activation::ReLU => "Rectified Linear Unit: max(0, x)",
                Activation::Sigmoid => "Sigmoid: 1/(1+e^-x), outputs 0-1",
                Activation::Tanh => "Hyperbolic tangent: outputs -1 to 1",
                Activation::Softmax => "Softmax: probability distribution",
                Activation::Linear => "Linear: f(x) = x",
                Activation::LeakyReLU(_) => "Leaky ReLU: x if x>0, else 0.01*x",
                Activation::Swish => "Swish: x * sigmoid(x)",
            }
            .to_string(),
        ),
    );

    Ok(Value::Record(info))
}

// --------------- Evolution Builtins ---------------

/// population(size, genome_type, [config])
/// Create a population for evolution
/// Args:
///   - size: Int - Population size
///   - genome_type: String - "nn" for neural network, "vec" for float vector
///   - config: Record (optional) - Evolution configuration
/// Returns: Record - Population state
fn bi_population(args: Vec<Value>, _input: Option<Value>, _env: &mut Env) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("population requires size and genome_type"));
    }

    let size = expect_int("population", &args[0])? as usize;
    let genome_type = expect_string("population", &args[1])?;

    let mut config = EvolutionConfig::default();
    config.population_size = size;

    // Parse optional config
    if let Some(Value::Record(cfg)) = args.get(2) {
        if let Some(Value::Float(r)) = cfg.get("mutation_rate") {
            config.mutation_rate = *r;
        }
        if let Some(Value::Float(s)) = cfg.get("mutation_strength") {
            config.mutation_strength = *s;
        }
        if let Some(Value::Float(c)) = cfg.get("crossover_rate") {
            config.crossover_rate = *c;
        }
        if let Some(Value::Int(e)) = cfg.get("elitism") {
            config.elitism = *e as usize;
        }
        if let Some(Value::Int(g)) = cfg.get("generations") {
            config.generations = *g as usize;
        }
    }

    // Create population based on genome type
    match genome_type {
        "nn" => {
            let pop: Population<NeuralNetwork> = Population::new(config.clone());
            population_to_value(&pop, "nn")
        }
        "vec" | "vector" => {
            let pop: Population<Vec<f64>> = Population::new(config.clone());
            population_to_value(&pop, "vec")
        }
        _ => Err(anyhow!(
            "Unknown genome_type: {}. Valid: nn, vec",
            genome_type
        )),
    }
}

/// evolve(population, fitness_fn, generations)
/// Run evolution for multiple generations
/// Args:
///   - population: Record - Population from population()
///   - fitness_fn: Lambda - fn(genome) => Float
///   - generations: Int - Number of generations
/// Returns: Record - Updated population with stats
fn bi_evolve(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let has_input = input.is_some();
    let pop_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("evolve requires population"))?;

    let idx = if has_input { 0 } else { 1 };
    let fitness_fn = args
        .get(idx)
        .ok_or_else(|| anyhow!("evolve requires fitness function"))?;
    let lambda = need_lambda(fitness_fn, "evolve")?.clone();

    let generations = args
        .get(idx + 1)
        .map(|v| expect_int("evolve", v))
        .transpose()?
        .unwrap_or(10) as usize;

    let genome_type = get_population_genome_type(&pop_val)?;

    match genome_type.as_str() {
        "nn" => {
            let mut pop = value_to_population_nn(&pop_val)?;
            for _ in 0..generations {
                pop.evolve(|genome| {
                    let genome_val = nn_to_value(genome).unwrap_or(Value::Null);
                    let result =
                        call_lambda(&lambda, &[genome_val], env).unwrap_or(Value::Float(0.0));
                    let score = value_to_f64(&result).unwrap_or(0.0);
                    FitnessResult::new(score)
                });
            }
            population_to_value(&pop, "nn")
        }
        "vec" => {
            let mut pop = value_to_population_vec(&pop_val)?;
            for _ in 0..generations {
                pop.evolve(|genome| {
                    let genome_val =
                        Value::Array(genome.iter().map(|&f| Value::Float(f)).collect());
                    let result =
                        call_lambda(&lambda, &[genome_val], env).unwrap_or(Value::Float(0.0));
                    let score = value_to_f64(&result).unwrap_or(0.0);
                    FitnessResult::new(score)
                });
            }
            population_to_value(&pop, "vec")
        }
        _ => Err(anyhow!("Unknown genome type: {}", genome_type)),
    }
}

/// evolve_step(population, fitness_fn)
/// Run single evolution step
fn bi_evolve_step(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let has_input = input.is_some();
    let pop_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("evolve_step requires population"))?;

    let idx = if has_input { 0 } else { 1 };
    let fitness_fn = args
        .get(idx)
        .ok_or_else(|| anyhow!("evolve_step requires fitness function"))?;
    let lambda = need_lambda(fitness_fn, "evolve_step")?.clone();

    let genome_type = get_population_genome_type(&pop_val)?;

    match genome_type.as_str() {
        "nn" => {
            let mut pop = value_to_population_nn(&pop_val)?;
            pop.evolve(|genome| {
                let genome_val = nn_to_value(genome).unwrap_or(Value::Null);
                let result = call_lambda(&lambda, &[genome_val], env).unwrap_or(Value::Float(0.0));
                let score = value_to_f64(&result).unwrap_or(0.0);
                FitnessResult::new(score)
            });
            population_to_value(&pop, "nn")
        }
        "vec" => {
            let mut pop = value_to_population_vec(&pop_val)?;
            pop.evolve(|genome| {
                let genome_val = Value::Array(genome.iter().map(|&f| Value::Float(f)).collect());
                let result = call_lambda(&lambda, &[genome_val], env).unwrap_or(Value::Float(0.0));
                let score = value_to_f64(&result).unwrap_or(0.0);
                FitnessResult::new(score)
            });
            population_to_value(&pop, "vec")
        }
        _ => Err(anyhow!("Unknown genome type: {}", genome_type)),
    }
}

/// fitness(population, fitness_fn)
/// Evaluate fitness of all individuals
fn bi_fitness(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let has_input = input.is_some();
    let pop_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("fitness requires population"))?;

    let idx = if has_input { 0 } else { 1 };
    let fitness_fn = args
        .get(idx)
        .ok_or_else(|| anyhow!("fitness requires fitness function"))?;
    let lambda = need_lambda(fitness_fn, "fitness")?.clone();

    let genome_type = get_population_genome_type(&pop_val)?;

    match genome_type.as_str() {
        "nn" => {
            let mut pop = value_to_population_nn(&pop_val)?;
            pop.evaluate(|genome| {
                let genome_val = nn_to_value(genome).unwrap_or(Value::Null);
                let result = call_lambda(&lambda, &[genome_val], env).unwrap_or(Value::Float(0.0));
                let score = value_to_f64(&result).unwrap_or(0.0);
                FitnessResult::new(score)
            });
            population_to_value(&pop, "nn")
        }
        "vec" => {
            let mut pop = value_to_population_vec(&pop_val)?;
            pop.evaluate(|genome| {
                let genome_val = Value::Array(genome.iter().map(|&f| Value::Float(f)).collect());
                let result = call_lambda(&lambda, &[genome_val], env).unwrap_or(Value::Float(0.0));
                let score = value_to_f64(&result).unwrap_or(0.0);
                FitnessResult::new(score)
            });
            population_to_value(&pop, "vec")
        }
        _ => Err(anyhow!("Unknown genome type: {}", genome_type)),
    }
}

/// best_individual(population)
/// Get the best individual from population
fn bi_best_individual(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let pop_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("best_individual requires population"))?;

    let genome_type = get_population_genome_type(&pop_val)?;

    match genome_type.as_str() {
        "nn" => {
            let pop = value_to_population_nn(&pop_val)?;
            let best = pop
                .individuals
                .iter()
                .max_by(|a, b| {
                    a.score()
                        .partial_cmp(&b.score())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .ok_or_else(|| anyhow!("Empty population"))?;

            let mut result = BTreeMap::new();
            result.insert("genome".to_string(), nn_to_value(&best.genome)?);
            result.insert("fitness".to_string(), Value::Float(best.score()));
            result.insert("generation".to_string(), Value::Int(best.generation as i64));
            result.insert("id".to_string(), Value::Int(best.id as i64));
            Ok(Value::Record(result))
        }
        "vec" => {
            let pop = value_to_population_vec(&pop_val)?;
            let best = pop
                .individuals
                .iter()
                .max_by(|a, b| {
                    a.score()
                        .partial_cmp(&b.score())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .ok_or_else(|| anyhow!("Empty population"))?;

            let mut result = BTreeMap::new();
            result.insert(
                "genome".to_string(),
                Value::Array(best.genome.iter().map(|&f| Value::Float(f)).collect()),
            );
            result.insert("fitness".to_string(), Value::Float(best.score()));
            result.insert("generation".to_string(), Value::Int(best.generation as i64));
            result.insert("id".to_string(), Value::Int(best.id as i64));
            Ok(Value::Record(result))
        }
        _ => Err(anyhow!("Unknown genome type: {}", genome_type)),
    }
}

/// evolution_stats(population)
/// Get evolution statistics
fn bi_evolution_stats(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let pop_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("evolution_stats requires population"))?;

    if let Value::Record(rec) = &pop_val {
        if let Some(Value::Record(stats)) = rec.get("stats") {
            return Ok(Value::Record(stats.clone()));
        }
    }

    Err(anyhow!("Could not extract stats from population"))
}

/// selection_strategy(name, [param])
/// Create selection strategy
/// Args:
///   - name: String - "tournament", "roulette", "rank", "truncation", "elite"
///   - param: Int/Float (optional) - Strategy parameter
fn bi_selection_strategy(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let name = args
        .get(0)
        .and_then(|v| {
            if let Value::Str(s) = v {
                Some(s.as_str())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("selection_strategy requires name"))?;

    let strategy = match name.to_lowercase().as_str() {
        "tournament" => {
            let size = args
                .get(1)
                .map(|v| expect_int("selection_strategy", v))
                .transpose()?
                .unwrap_or(3) as usize;
            SelectionStrategy::Tournament(size)
        }
        "roulette" => SelectionStrategy::Roulette,
        "rank" => SelectionStrategy::Rank,
        "truncation" => {
            let ratio = args.get(1).map(value_to_f64).transpose()?.unwrap_or(0.5);
            SelectionStrategy::Truncation(ratio)
        }
        "elite" => {
            let n = args
                .get(1)
                .map(|v| expect_int("selection_strategy", v))
                .transpose()?
                .unwrap_or(5) as usize;
            SelectionStrategy::Elite(n)
        }
        _ => return Err(anyhow!("Unknown selection strategy: {}", name)),
    };

    let mut result = BTreeMap::new();
    result.insert(
        "type".to_string(),
        Value::Str("selection_strategy".to_string()),
    );
    result.insert("name".to_string(), Value::Str(format!("{:?}", strategy)));
    Ok(Value::Record(result))
}

/// crossover_strategy(name, [param])
/// Create crossover strategy
fn bi_crossover_strategy(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let name = args
        .get(0)
        .and_then(|v| {
            if let Value::Str(s) = v {
                Some(s.as_str())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("crossover_strategy requires name"))?;

    let strategy = match name.to_lowercase().as_str() {
        "single_point" | "singlepoint" => CrossoverStrategy::SinglePoint,
        "two_point" | "twopoint" => CrossoverStrategy::TwoPoint,
        "uniform" => {
            let prob = args.get(1).map(value_to_f64).transpose()?.unwrap_or(0.5);
            CrossoverStrategy::Uniform(prob)
        }
        "blend" => {
            let alpha = args.get(1).map(value_to_f64).transpose()?.unwrap_or(0.5);
            CrossoverStrategy::Blend(alpha)
        }
        "sbx" => {
            let eta = args.get(1).map(value_to_f64).transpose()?.unwrap_or(20.0);
            CrossoverStrategy::SBX(eta)
        }
        _ => return Err(anyhow!("Unknown crossover strategy: {}", name)),
    };

    let mut result = BTreeMap::new();
    result.insert(
        "type".to_string(),
        Value::Str("crossover_strategy".to_string()),
    );
    result.insert("name".to_string(), Value::Str(format!("{:?}", strategy)));
    Ok(Value::Record(result))
}

/// evolution_config([options])
/// Create evolution configuration
fn bi_evolution_config(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let mut config = EvolutionConfig::default();

    if let Some(Value::Record(opts)) = args.get(0) {
        if let Some(v) = opts.get("population_size") {
            config.population_size = expect_int("evolution_config", v)? as usize;
        }
        if let Some(v) = opts.get("generations") {
            config.generations = expect_int("evolution_config", v)? as usize;
        }
        if let Some(v) = opts.get("mutation_rate") {
            config.mutation_rate = value_to_f64(v)?;
        }
        if let Some(v) = opts.get("mutation_strength") {
            config.mutation_strength = value_to_f64(v)?;
        }
        if let Some(v) = opts.get("crossover_rate") {
            config.crossover_rate = value_to_f64(v)?;
        }
        if let Some(v) = opts.get("elitism") {
            config.elitism = expect_int("evolution_config", v)? as usize;
        }
    }

    let mut result = BTreeMap::new();
    result.insert(
        "type".to_string(),
        Value::Str("evolution_config".to_string()),
    );
    result.insert(
        "population_size".to_string(),
        Value::Int(config.population_size as i64),
    );
    result.insert(
        "generations".to_string(),
        Value::Int(config.generations as i64),
    );
    result.insert(
        "mutation_rate".to_string(),
        Value::Float(config.mutation_rate),
    );
    result.insert(
        "mutation_strength".to_string(),
        Value::Float(config.mutation_strength),
    );
    result.insert(
        "crossover_rate".to_string(),
        Value::Float(config.crossover_rate),
    );
    result.insert("elitism".to_string(), Value::Int(config.elitism as i64));
    Ok(Value::Record(result))
}

/// coevolve(populations, fitness_fn, generations)
/// Run coevolution across multiple populations
fn bi_coevolve(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let has_input = input.is_some();
    let pops_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("coevolve requires populations array"))?;

    let pops_arr = expect_array("coevolve", &pops_val)?;

    let idx = if has_input { 0 } else { 1 };
    let fitness_fn = args
        .get(idx)
        .ok_or_else(|| anyhow!("coevolve requires fitness function"))?;
    let lambda = need_lambda(fitness_fn, "coevolve")?.clone();

    let generations = args
        .get(idx + 1)
        .map(|v| expect_int("coevolve", v))
        .transpose()?
        .unwrap_or(10) as usize;

    // For simplicity, evolve each population with fitness relative to others
    let mut results = Vec::new();
    for pop_val in pops_arr {
        let genome_type = get_population_genome_type(pop_val)?;

        match genome_type.as_str() {
            "nn" => {
                let mut pop = value_to_population_nn(pop_val)?;
                for _ in 0..generations {
                    pop.evolve(|genome| {
                        let genome_val = nn_to_value(genome).unwrap_or(Value::Null);
                        let result =
                            call_lambda(&lambda, &[genome_val], env).unwrap_or(Value::Float(0.0));
                        let score = value_to_f64(&result).unwrap_or(0.0);
                        FitnessResult::new(score)
                    });
                }
                results.push(population_to_value(&pop, "nn")?);
            }
            "vec" => {
                let mut pop = value_to_population_vec(pop_val)?;
                for _ in 0..generations {
                    pop.evolve(|genome| {
                        let genome_val =
                            Value::Array(genome.iter().map(|&f| Value::Float(f)).collect());
                        let result =
                            call_lambda(&lambda, &[genome_val], env).unwrap_or(Value::Float(0.0));
                        let score = value_to_f64(&result).unwrap_or(0.0);
                        FitnessResult::new(score)
                    });
                }
                results.push(population_to_value(&pop, "vec")?);
            }
            _ => return Err(anyhow!("Unknown genome type: {}", genome_type)),
        }
    }

    Ok(Value::Array(results))
}

// --------------- Neural/Evolution Helpers ---------------

fn value_to_f64(v: &Value) -> Result<f64> {
    match v {
        Value::Float(f) => Ok(*f),
        Value::Int(i) => Ok(*i as f64),
        _ => Err(anyhow!("Expected numeric value, got {:?}", v)),
    }
}

fn nn_to_value(network: &NeuralNetwork) -> Result<Value> {
    let json = serde_json::to_string(network).context("Failed to serialize network")?;

    let mut rec = BTreeMap::new();
    rec.insert("type".to_string(), Value::Str("neural_network".to_string()));
    rec.insert("name".to_string(), Value::Str(network.name.clone()));
    rec.insert(
        "layer_count".to_string(),
        Value::Int(network.layers.len() as i64),
    );
    rec.insert(
        "param_count".to_string(),
        Value::Int(network.param_count() as i64),
    );
    rec.insert("_data".to_string(), Value::Str(json));
    Ok(Value::Record(rec))
}

fn value_to_nn(v: &Value) -> Result<NeuralNetwork> {
    if let Value::Record(rec) = v {
        if let Some(Value::Str(data)) = rec.get("_data") {
            return serde_json::from_str(data).context("Failed to deserialize network");
        }
    }
    Err(anyhow!("Invalid neural network value"))
}

fn consensus_net_to_value(network: &ConsensusNetwork) -> Result<Value> {
    let json = serde_json::to_string(network).context("Failed to serialize consensus network")?;

    let mut rec = BTreeMap::new();
    rec.insert(
        "type".to_string(),
        Value::Str("consensus_network".to_string()),
    );
    rec.insert("name".to_string(), Value::Str(network.name.clone()));
    rec.insert(
        "agent_count".to_string(),
        Value::Int(network.agent_networks.len() as i64),
    );
    rec.insert("_data".to_string(), Value::Str(json));
    Ok(Value::Record(rec))
}

fn value_to_consensus_net(v: &Value) -> Result<ConsensusNetwork> {
    if let Value::Record(rec) = v {
        if let Some(Value::Str(data)) = rec.get("_data") {
            return serde_json::from_str(data).context("Failed to deserialize consensus network");
        }
    }
    Err(anyhow!("Invalid consensus network value"))
}

fn get_population_genome_type(v: &Value) -> Result<String> {
    if let Value::Record(rec) = v {
        if let Some(Value::Str(gt)) = rec.get("genome_type") {
            return Ok(gt.clone());
        }
    }
    Err(anyhow!("Could not determine genome type from population"))
}

fn population_to_value<G: crate::evolution::Evolvable + serde::Serialize>(
    pop: &Population<G>,
    genome_type: &str,
) -> Result<Value> {
    let json = serde_json::to_string(&pop.individuals).context("Failed to serialize population")?;
    let config_json = serde_json::to_string(&pop.config).context("Failed to serialize config")?;

    let mut stats = BTreeMap::new();
    stats.insert(
        "generation".to_string(),
        Value::Int(pop.stats.generation as i64),
    );
    stats.insert(
        "best_fitness".to_string(),
        Value::Float(pop.stats.best_fitness),
    );
    stats.insert(
        "avg_fitness".to_string(),
        Value::Float(pop.stats.avg_fitness),
    );
    stats.insert(
        "worst_fitness".to_string(),
        Value::Float(pop.stats.worst_fitness),
    );
    stats.insert("diversity".to_string(), Value::Float(pop.stats.diversity));
    stats.insert(
        "history".to_string(),
        Value::Array(
            pop.stats
                .fitness_history
                .iter()
                .map(|&f| Value::Float(f))
                .collect(),
        ),
    );

    let mut rec = BTreeMap::new();
    rec.insert("type".to_string(), Value::Str("population".to_string()));
    rec.insert(
        "genome_type".to_string(),
        Value::Str(genome_type.to_string()),
    );
    rec.insert("size".to_string(), Value::Int(pop.individuals.len() as i64));
    rec.insert("generation".to_string(), Value::Int(pop.generation as i64));
    rec.insert("stats".to_string(), Value::Record(stats));
    rec.insert("_individuals".to_string(), Value::Str(json));
    rec.insert("_config".to_string(), Value::Str(config_json));
    Ok(Value::Record(rec))
}

fn value_to_population_nn(v: &Value) -> Result<Population<NeuralNetwork>> {
    if let Value::Record(rec) = v {
        let individuals: Vec<crate::evolution::Individual<NeuralNetwork>> =
            if let Some(Value::Str(data)) = rec.get("_individuals") {
                serde_json::from_str(data).context("Failed to deserialize individuals")?
            } else {
                return Err(anyhow!("Missing _individuals in population"));
            };

        let config: EvolutionConfig = if let Some(Value::Str(data)) = rec.get("_config") {
            serde_json::from_str(data).context("Failed to deserialize config")?
        } else {
            EvolutionConfig::default()
        };

        let generation = if let Some(Value::Int(g)) = rec.get("generation") {
            *g as usize
        } else {
            0
        };

        Ok(Population {
            individuals,
            generation,
            config,
            stats: crate::evolution::EvolutionStats::new(),
            next_id: 0,
        })
    } else {
        Err(anyhow!("Invalid population value"))
    }
}

fn value_to_population_vec(v: &Value) -> Result<Population<Vec<f64>>> {
    if let Value::Record(rec) = v {
        let individuals: Vec<crate::evolution::Individual<Vec<f64>>> =
            if let Some(Value::Str(data)) = rec.get("_individuals") {
                serde_json::from_str(data).context("Failed to deserialize individuals")?
            } else {
                return Err(anyhow!("Missing _individuals in population"));
            };

        let config: EvolutionConfig = if let Some(Value::Str(data)) = rec.get("_config") {
            serde_json::from_str(data).context("Failed to deserialize config")?
        } else {
            EvolutionConfig::default()
        };

        let generation = if let Some(Value::Int(g)) = rec.get("generation") {
            *g as usize
        } else {
            0
        };

        Ok(Population {
            individuals,
            generation,
            config,
            stats: crate::evolution::EvolutionStats::new(),
            next_id: 0,
        })
    } else {
        Err(anyhow!("Invalid population value"))
    }
}

// ==================== Reinforcement Learning Builtins ====================

/// Create a Q-learning agent
/// Usage: rl_agent(name, state_size, action_size, [config])
fn bi_rl_agent(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.is_empty() {
        return Err(anyhow!(
            "rl_agent requires: name, state_size, action_size, [config]"
        ));
    }

    let name = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(anyhow!("rl_agent: name must be a string")),
    };

    let state_size = match args.get(1) {
        Some(Value::Int(n)) => *n as usize,
        _ => return Err(anyhow!("rl_agent: state_size must be an integer")),
    };

    let action_size = match args.get(2) {
        Some(Value::Int(n)) => *n as usize,
        _ => return Err(anyhow!("rl_agent: action_size must be an integer")),
    };

    // Parse optional config
    let (learning_rate, discount, epsilon) = if let Some(Value::Record(config)) = args.get(3) {
        (
            config
                .get("learning_rate")
                .and_then(|v| match v {
                    Value::Float(f) => Some(*f),
                    _ => None,
                })
                .unwrap_or(0.1),
            config
                .get("discount")
                .and_then(|v| match v {
                    Value::Float(f) => Some(*f),
                    _ => None,
                })
                .unwrap_or(0.99),
            config
                .get("epsilon")
                .and_then(|v| match v {
                    Value::Float(f) => Some(*f),
                    _ => None,
                })
                .unwrap_or(0.1),
        )
    } else {
        (0.1, 0.99, 0.1)
    };

    let agent = QLearningAgent::new(
        &name,
        state_size,
        action_size,
        learning_rate,
        discount,
        epsilon,
    );
    q_agent_to_value(&agent)
}

/// Select action using Q-learning agent
/// Usage: rl_action(agent, state)
fn bi_rl_action(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.len() < 2 {
        return Err(anyhow!("rl_action requires: agent, state"));
    }

    let agent = value_to_q_agent(&args[0])?;

    let state = match &args[1] {
        Value::Int(s) => *s as usize,
        _ => return Err(anyhow!("rl_action: state must be an integer")),
    };

    let action = agent.select_action(state);
    Ok(Value::Int(action as i64))
}

/// Update Q-learning agent
/// Usage: rl_update(agent, state, action, reward, next_state, done)
fn bi_rl_update(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.len() < 6 {
        return Err(anyhow!(
            "rl_update requires: agent, state, action, reward, next_state, done"
        ));
    }

    let mut agent = value_to_q_agent(&args[0])?;

    let state = match &args[1] {
        Value::Int(s) => *s as usize,
        _ => return Err(anyhow!("state must be integer")),
    };

    let action = match &args[2] {
        Value::Int(a) => *a as usize,
        _ => return Err(anyhow!("action must be integer")),
    };

    let reward = match &args[3] {
        Value::Float(r) => *r,
        Value::Int(r) => *r as f64,
        _ => return Err(anyhow!("reward must be number")),
    };

    let next_state = match &args[4] {
        Value::Int(s) => *s as usize,
        _ => return Err(anyhow!("next_state must be integer")),
    };

    let done = match &args[5] {
        Value::Bool(b) => *b,
        _ => return Err(anyhow!("done must be boolean")),
    };

    agent.update(state, action, reward, next_state, done);
    q_agent_to_value(&agent)
}

/// Create a SARSA agent
fn bi_rl_sarsa_agent(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.len() < 3 {
        return Err(anyhow!(
            "rl_sarsa_agent requires: name, state_size, action_size"
        ));
    }

    let name = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(anyhow!("name must be string")),
    };

    let state_size = match &args[1] {
        Value::Int(n) => *n as usize,
        _ => return Err(anyhow!("state_size must be integer")),
    };

    let action_size = match &args[2] {
        Value::Int(n) => *n as usize,
        _ => return Err(anyhow!("action_size must be integer")),
    };

    let agent = SarsaAgent::new(&name, state_size, action_size, 0.1, 0.99, 0.1);
    sarsa_agent_to_value(&agent)
}

/// Update SARSA agent
fn bi_rl_sarsa_update(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.len() < 7 {
        return Err(anyhow!(
            "rl_sarsa_update requires: agent, state, action, reward, next_state, next_action, done"
        ));
    }

    let mut agent = value_to_sarsa_agent(&args[0])?;

    let state = match &args[1] {
        Value::Int(s) => *s as usize,
        _ => return Err(anyhow!("state must be integer")),
    };

    let action = match &args[2] {
        Value::Int(a) => *a as usize,
        _ => return Err(anyhow!("action must be integer")),
    };

    let reward = match &args[3] {
        Value::Float(r) => *r,
        Value::Int(r) => *r as f64,
        _ => return Err(anyhow!("reward must be number")),
    };

    let next_state = match &args[4] {
        Value::Int(s) => *s as usize,
        _ => return Err(anyhow!("next_state must be integer")),
    };

    let next_action = match &args[5] {
        Value::Int(a) => *a as usize,
        _ => return Err(anyhow!("next_action must be integer")),
    };

    let done = match &args[6] {
        Value::Bool(b) => *b,
        _ => return Err(anyhow!("done must be boolean")),
    };

    agent.update(state, action, reward, next_state, next_action, done);
    sarsa_agent_to_value(&agent)
}

/// Create a Policy Gradient agent
fn bi_rl_pg_agent(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.len() < 3 {
        return Err(anyhow!(
            "rl_pg_agent requires: name, state_dim, action_size"
        ));
    }

    let name = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(anyhow!("name must be string")),
    };

    let state_dim = match &args[1] {
        Value::Int(n) => *n as usize,
        _ => return Err(anyhow!("state_dim must be integer")),
    };

    let action_size = match &args[2] {
        Value::Int(n) => *n as usize,
        _ => return Err(anyhow!("action_size must be integer")),
    };

    let agent = PolicyGradientAgent::new(&name, state_dim, action_size, 0.01, 0.99);
    pg_agent_to_value(&agent)
}

/// Record step for Policy Gradient agent
fn bi_rl_pg_step(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.len() < 4 {
        return Err(anyhow!("rl_pg_step requires: agent, state, action, reward"));
    }

    let mut agent = value_to_pg_agent(&args[0])?;

    let state: Vec<f64> = match &args[1] {
        Value::Array(arr) => arr
            .iter()
            .map(|v| match v {
                Value::Float(f) => Ok(*f),
                Value::Int(i) => Ok(*i as f64),
                _ => Err(anyhow!("state elements must be numbers")),
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err(anyhow!("state must be array")),
    };

    let action = match &args[2] {
        Value::Int(a) => *a as usize,
        _ => return Err(anyhow!("action must be integer")),
    };

    let reward = match &args[3] {
        Value::Float(r) => *r,
        Value::Int(r) => *r as f64,
        _ => return Err(anyhow!("reward must be number")),
    };

    agent.record_step(state, action, reward);
    pg_agent_to_value(&agent)
}

/// End episode for Policy Gradient agent (triggers update)
fn bi_rl_pg_episode_end(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.is_empty() {
        return Err(anyhow!("rl_pg_episode_end requires: agent"));
    }

    let mut agent = value_to_pg_agent(&args[0])?;
    agent.end_episode();
    pg_agent_to_value(&agent)
}

/// Create an Actor-Critic agent
fn bi_rl_ac_agent(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.len() < 3 {
        return Err(anyhow!(
            "rl_ac_agent requires: name, state_dim, action_size"
        ));
    }

    let name = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(anyhow!("name must be string")),
    };

    let state_dim = match &args[1] {
        Value::Int(n) => *n as usize,
        _ => return Err(anyhow!("state_dim must be integer")),
    };

    let action_size = match &args[2] {
        Value::Int(n) => *n as usize,
        _ => return Err(anyhow!("action_size must be integer")),
    };

    let agent = ActorCriticAgent::new(&name, state_dim, action_size, 0.01, 0.01, 0.99);
    ac_agent_to_value(&agent)
}

/// Update Actor-Critic agent
fn bi_rl_ac_update(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.len() < 6 {
        return Err(anyhow!(
            "rl_ac_update requires: agent, state, action, reward, next_state, done"
        ));
    }

    let mut agent = value_to_ac_agent(&args[0])?;

    let state: Vec<f64> = match &args[1] {
        Value::Array(arr) => arr
            .iter()
            .map(|v| match v {
                Value::Float(f) => Ok(*f),
                Value::Int(i) => Ok(*i as f64),
                _ => Err(anyhow!("state elements must be numbers")),
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err(anyhow!("state must be array")),
    };

    let action = match &args[2] {
        Value::Int(a) => *a as usize,
        _ => return Err(anyhow!("action must be integer")),
    };

    let reward = match &args[3] {
        Value::Float(r) => *r,
        Value::Int(r) => *r as f64,
        _ => return Err(anyhow!("reward must be number")),
    };

    let next_state: Vec<f64> = match &args[4] {
        Value::Array(arr) => arr
            .iter()
            .map(|v| match v {
                Value::Float(f) => Ok(*f),
                Value::Int(i) => Ok(*i as f64),
                _ => Err(anyhow!("next_state elements must be numbers")),
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err(anyhow!("next_state must be array")),
    };

    let done = match &args[5] {
        Value::Bool(b) => *b,
        _ => return Err(anyhow!("done must be boolean")),
    };

    agent.update(&state, action, reward, &next_state, done);
    ac_agent_to_value(&agent)
}

/// Create a DQN agent
fn bi_rl_dqn_agent(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.len() < 3 {
        return Err(anyhow!(
            "rl_dqn_agent requires: name, state_dim, action_size, [hidden_sizes]"
        ));
    }

    let name = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(anyhow!("name must be string")),
    };

    let state_dim = match &args[1] {
        Value::Int(n) => *n as usize,
        _ => return Err(anyhow!("state_dim must be integer")),
    };

    let action_size = match &args[2] {
        Value::Int(n) => *n as usize,
        _ => return Err(anyhow!("action_size must be integer")),
    };

    let hidden_sizes: Vec<usize> = if let Some(Value::Array(arr)) = args.get(3) {
        arr.iter()
            .map(|v| match v {
                Value::Int(n) => Ok(*n as usize),
                _ => Err(anyhow!("hidden_sizes must be array of integers")),
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        vec![64, 64] // Default hidden layers
    };

    let agent = DQNAgent::new(
        &name,
        state_dim,
        action_size,
        &hidden_sizes,
        0.001, // learning rate
        0.99,  // discount
        1.0,   // epsilon
        10000, // buffer size
    );
    dqn_agent_to_value(&agent)
}

/// Step DQN agent (store experience and train)
fn bi_rl_dqn_step(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.len() < 6 {
        return Err(anyhow!(
            "rl_dqn_step requires: agent, state, action, reward, next_state, done"
        ));
    }

    let mut agent = value_to_dqn_agent(&args[0])?;

    let state: Vec<f64> = match &args[1] {
        Value::Array(arr) => arr
            .iter()
            .map(|v| match v {
                Value::Float(f) => Ok(*f),
                Value::Int(i) => Ok(*i as f64),
                _ => Err(anyhow!("state elements must be numbers")),
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err(anyhow!("state must be array")),
    };

    let action = match &args[2] {
        Value::Int(a) => *a as usize,
        _ => return Err(anyhow!("action must be integer")),
    };

    let reward = match &args[3] {
        Value::Float(r) => *r,
        Value::Int(r) => *r as f64,
        _ => return Err(anyhow!("reward must be number")),
    };

    let next_state: Vec<f64> = match &args[4] {
        Value::Array(arr) => arr
            .iter()
            .map(|v| match v {
                Value::Float(f) => Ok(*f),
                Value::Int(i) => Ok(*i as f64),
                _ => Err(anyhow!("next_state elements must be numbers")),
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err(anyhow!("next_state must be array")),
    };

    let done = match &args[5] {
        Value::Bool(b) => *b,
        _ => return Err(anyhow!("done must be boolean")),
    };

    agent.step(state, action, reward, next_state, done);
    dqn_agent_to_value(&agent)
}

/// Create a replay buffer
fn bi_rl_replay_buffer(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    let capacity = match args.first() {
        Some(Value::Int(n)) => *n as usize,
        _ => 10000, // Default capacity
    };

    let buffer = ReplayBuffer::new(capacity);
    replay_buffer_to_value(&buffer)
}

/// Create a gridworld environment
fn bi_rl_gridworld(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    let width = match args.first() {
        Some(Value::Int(n)) => *n as usize,
        _ => 5,
    };

    let height = match args.get(1) {
        Some(Value::Int(n)) => *n as usize,
        _ => width,
    };

    let env = GridWorld::new(width, height);
    gridworld_to_value(&env)
}

/// Step gridworld environment
fn bi_rl_env_step(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let args = if args.is_empty() && input.is_some() {
        vec![input.unwrap()]
    } else {
        args
    };

    if args.len() < 2 {
        return Err(anyhow!("rl_env_step requires: env, action"));
    }

    let mut env = value_to_gridworld(&args[0])?;

    let action = match &args[1] {
        Value::Int(a) => *a as usize,
        _ => return Err(anyhow!("action must be integer")),
    };

    let (next_state, reward, done) = env.step(action);

    let mut result = BTreeMap::new();
    result.insert("state".to_string(), Value::Int(next_state as i64));
    result.insert("reward".to_string(), Value::Float(reward));
    result.insert("done".to_string(), Value::Bool(done));
    result.insert("env".to_string(), gridworld_to_value(&env)?);

    Ok(Value::Record(result))
}

// ==================== RL Helper Functions ====================

fn q_agent_to_value(agent: &QLearningAgent) -> Result<Value> {
    let json = serde_json::to_string(agent).context("Failed to serialize Q-learning agent")?;

    let mut rec = BTreeMap::new();
    rec.insert(
        "type".to_string(),
        Value::Str("q_learning_agent".to_string()),
    );
    rec.insert("name".to_string(), Value::Str(agent.name.clone()));
    rec.insert(
        "state_size".to_string(),
        Value::Int(agent.state_size as i64),
    );
    rec.insert(
        "action_size".to_string(),
        Value::Int(agent.action_size as i64),
    );
    rec.insert("epsilon".to_string(), Value::Float(agent.epsilon));
    rec.insert("steps".to_string(), Value::Int(agent.steps as i64));
    rec.insert("_data".to_string(), Value::Str(json));

    Ok(Value::Record(rec))
}

fn value_to_q_agent(v: &Value) -> Result<QLearningAgent> {
    if let Value::Record(rec) = v {
        if let Some(Value::Str(data)) = rec.get("_data") {
            serde_json::from_str(data).context("Failed to deserialize Q-learning agent")
        } else {
            Err(anyhow!("Invalid Q-learning agent: missing _data"))
        }
    } else {
        Err(anyhow!("Invalid Q-learning agent value"))
    }
}

fn sarsa_agent_to_value(agent: &SarsaAgent) -> Result<Value> {
    let json = serde_json::to_string(agent).context("Failed to serialize SARSA agent")?;

    let mut rec = BTreeMap::new();
    rec.insert("type".to_string(), Value::Str("sarsa_agent".to_string()));
    rec.insert("name".to_string(), Value::Str(agent.name.clone()));
    rec.insert(
        "state_size".to_string(),
        Value::Int(agent.state_size as i64),
    );
    rec.insert(
        "action_size".to_string(),
        Value::Int(agent.action_size as i64),
    );
    rec.insert("epsilon".to_string(), Value::Float(agent.epsilon));
    rec.insert("steps".to_string(), Value::Int(agent.steps as i64));
    rec.insert("_data".to_string(), Value::Str(json));

    Ok(Value::Record(rec))
}

fn value_to_sarsa_agent(v: &Value) -> Result<SarsaAgent> {
    if let Value::Record(rec) = v {
        if let Some(Value::Str(data)) = rec.get("_data") {
            serde_json::from_str(data).context("Failed to deserialize SARSA agent")
        } else {
            Err(anyhow!("Invalid SARSA agent: missing _data"))
        }
    } else {
        Err(anyhow!("Invalid SARSA agent value"))
    }
}

fn pg_agent_to_value(agent: &PolicyGradientAgent) -> Result<Value> {
    let json = serde_json::to_string(agent).context("Failed to serialize PG agent")?;

    let mut rec = BTreeMap::new();
    rec.insert(
        "type".to_string(),
        Value::Str("policy_gradient_agent".to_string()),
    );
    rec.insert("name".to_string(), Value::Str(agent.name.clone()));
    rec.insert("state_dim".to_string(), Value::Int(agent.state_dim as i64));
    rec.insert(
        "action_size".to_string(),
        Value::Int(agent.action_size as i64),
    );
    rec.insert("episodes".to_string(), Value::Int(agent.episodes as i64));
    rec.insert("_data".to_string(), Value::Str(json));

    Ok(Value::Record(rec))
}

fn value_to_pg_agent(v: &Value) -> Result<PolicyGradientAgent> {
    if let Value::Record(rec) = v {
        if let Some(Value::Str(data)) = rec.get("_data") {
            serde_json::from_str(data).context("Failed to deserialize PG agent")
        } else {
            Err(anyhow!("Invalid PG agent: missing _data"))
        }
    } else {
        Err(anyhow!("Invalid PG agent value"))
    }
}

fn ac_agent_to_value(agent: &ActorCriticAgent) -> Result<Value> {
    let json = serde_json::to_string(agent).context("Failed to serialize AC agent")?;

    let mut rec = BTreeMap::new();
    rec.insert(
        "type".to_string(),
        Value::Str("actor_critic_agent".to_string()),
    );
    rec.insert("name".to_string(), Value::Str(agent.name.clone()));
    rec.insert("state_dim".to_string(), Value::Int(agent.state_dim as i64));
    rec.insert(
        "action_size".to_string(),
        Value::Int(agent.action_size as i64),
    );
    rec.insert("steps".to_string(), Value::Int(agent.steps as i64));
    rec.insert("_data".to_string(), Value::Str(json));

    Ok(Value::Record(rec))
}

fn value_to_ac_agent(v: &Value) -> Result<ActorCriticAgent> {
    if let Value::Record(rec) = v {
        if let Some(Value::Str(data)) = rec.get("_data") {
            serde_json::from_str(data).context("Failed to deserialize AC agent")
        } else {
            Err(anyhow!("Invalid AC agent: missing _data"))
        }
    } else {
        Err(anyhow!("Invalid AC agent value"))
    }
}

fn dqn_agent_to_value(agent: &DQNAgent) -> Result<Value> {
    let json = serde_json::to_string(agent).context("Failed to serialize DQN agent")?;

    let mut rec = BTreeMap::new();
    rec.insert("type".to_string(), Value::Str("dqn_agent".to_string()));
    rec.insert("name".to_string(), Value::Str(agent.name.clone()));
    rec.insert("state_dim".to_string(), Value::Int(agent.state_dim as i64));
    rec.insert(
        "action_size".to_string(),
        Value::Int(agent.action_size as i64),
    );
    rec.insert("epsilon".to_string(), Value::Float(agent.epsilon));
    rec.insert("steps".to_string(), Value::Int(agent.steps as i64));
    rec.insert(
        "buffer_size".to_string(),
        Value::Int(agent.replay_buffer.len() as i64),
    );
    rec.insert("_data".to_string(), Value::Str(json));

    Ok(Value::Record(rec))
}

fn value_to_dqn_agent(v: &Value) -> Result<DQNAgent> {
    if let Value::Record(rec) = v {
        if let Some(Value::Str(data)) = rec.get("_data") {
            serde_json::from_str(data).context("Failed to deserialize DQN agent")
        } else {
            Err(anyhow!("Invalid DQN agent: missing _data"))
        }
    } else {
        Err(anyhow!("Invalid DQN agent value"))
    }
}

fn replay_buffer_to_value(buffer: &ReplayBuffer) -> Result<Value> {
    let json = serde_json::to_string(buffer).context("Failed to serialize replay buffer")?;

    let mut rec = BTreeMap::new();
    rec.insert("type".to_string(), Value::Str("replay_buffer".to_string()));
    rec.insert("capacity".to_string(), Value::Int(buffer.capacity as i64));
    rec.insert("size".to_string(), Value::Int(buffer.len() as i64));
    rec.insert("_data".to_string(), Value::Str(json));

    Ok(Value::Record(rec))
}

fn gridworld_to_value(env: &GridWorld) -> Result<Value> {
    let mut rec = BTreeMap::new();
    rec.insert("type".to_string(), Value::Str("gridworld".to_string()));
    rec.insert("width".to_string(), Value::Int(env.width as i64));
    rec.insert("height".to_string(), Value::Int(env.height as i64));
    rec.insert("agent_x".to_string(), Value::Int(env.agent_pos.0 as i64));
    rec.insert("agent_y".to_string(), Value::Int(env.agent_pos.1 as i64));
    rec.insert("goal_x".to_string(), Value::Int(env.goal_pos.0 as i64));
    rec.insert("goal_y".to_string(), Value::Int(env.goal_pos.1 as i64));
    rec.insert(
        "state_size".to_string(),
        Value::Int(env.state_size() as i64),
    );
    rec.insert(
        "action_size".to_string(),
        Value::Int(env.action_size() as i64),
    );

    Ok(Value::Record(rec))
}

fn value_to_gridworld(v: &Value) -> Result<GridWorld> {
    if let Value::Record(rec) = v {
        let width = match rec.get("width") {
            Some(Value::Int(n)) => *n as usize,
            _ => return Err(anyhow!("Invalid gridworld: missing width")),
        };

        let height = match rec.get("height") {
            Some(Value::Int(n)) => *n as usize,
            _ => return Err(anyhow!("Invalid gridworld: missing height")),
        };

        let agent_x = match rec.get("agent_x") {
            Some(Value::Int(n)) => *n as usize,
            _ => 0,
        };

        let agent_y = match rec.get("agent_y") {
            Some(Value::Int(n)) => *n as usize,
            _ => 0,
        };

        let mut env = GridWorld::new(width, height);
        env.agent_pos = (agent_x, agent_y);

        Ok(env)
    } else {
        Err(anyhow!("Invalid gridworld value"))
    }
}

// ===========================================================================
// OS Tools Builtins
// ===========================================================================

/// List all available tools or filter by category/OS
/// Usage: tools() | tools("network") | tools("linux")
fn bi_tools(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let db = OSToolsDatabase::new();

    // Get filter if provided
    let filter = args.first().and_then(|v| {
        if let Value::Str(s) = v {
            Some(s.to_lowercase())
        } else {
            None
        }
    });

    let tools: Vec<&crate::os_tools::OSTool> = if let Some(ref f) = filter {
        // Check if it's an OS filter
        let os_filter = match f.as_str() {
            "linux" => Some(OperatingSystem::Linux),
            "bsd" | "freebsd" | "openbsd" | "netbsd" => Some(OperatingSystem::BSD),
            "macos" | "mac" | "darwin" => Some(OperatingSystem::MacOS),
            "windows" | "win" => Some(OperatingSystem::Windows),
            "ios" | "iphone" => Some(OperatingSystem::iOS),
            "android" => Some(OperatingSystem::Android),
            _ => None,
        };

        if let Some(os) = os_filter {
            db.get_tools_by_os(&os)
        } else {
            // Check if it's a category filter
            let cat_filter = match f.as_str() {
                "filesystem" | "file" | "files" => Some(ToolCategory::FileSystem),
                "text" | "textprocessing" => Some(ToolCategory::TextProcessing),
                "network" | "net" => Some(ToolCategory::NetworkTools),
                "system" | "systeminfo" | "sysinfo" => Some(ToolCategory::SystemInfo),
                "process" | "processes" => Some(ToolCategory::ProcessManagement),
                "archive" | "archives" | "compression" => Some(ToolCategory::Archives),
                "search" => Some(ToolCategory::SearchTools),
                "monitor" | "monitoring" => Some(ToolCategory::Monitoring),
                "dev" | "development" => Some(ToolCategory::Development),
                "media" => Some(ToolCategory::Media),
                "security" | "sec" => Some(ToolCategory::Security),
                "util" | "utilities" => Some(ToolCategory::Utilities),
                "web" | "webtools" => Some(ToolCategory::WebTools),
                "cyber" | "cybersecurity" | "pentest" => Some(ToolCategory::CyberSecurity),
                "recon" | "reconnaissance" | "osint" => Some(ToolCategory::Reconnaissance),
                "forensics" | "forensic" => Some(ToolCategory::Forensics),
                "crypto" | "cryptography" => Some(ToolCategory::Cryptography),
                _ => None,
            };

            if let Some(cat) = cat_filter {
                db.get_tools_by_category(&cat)
            } else {
                // Fallback to search
                db.search_tools(f)
            }
        }
    } else {
        // Return all tools
        db.tools.values().collect()
    };

    // Convert to Value::Array of Records
    let tool_values: Vec<Value> = tools
        .iter()
        .map(|t| {
            let mut rec = BTreeMap::new();
            rec.insert("name".to_string(), Value::Str(t.name.clone()));
            rec.insert("description".to_string(), Value::Str(t.description.clone()));
            rec.insert("command".to_string(), Value::Str(t.command.clone()));
            rec.insert(
                "category".to_string(),
                Value::Str(format!("{:?}", t.category)),
            );
            rec.insert(
                "safety".to_string(),
                Value::Str(format!("{:?}", t.safety_level)),
            );
            rec.insert("requires_admin".to_string(), Value::Bool(t.requires_admin));
            rec.insert(
                "supported_os".to_string(),
                Value::Array(
                    t.supported_os
                        .iter()
                        .map(|os| Value::Str(format!("{:?}", os)))
                        .collect(),
                ),
            );
            Value::Record(rec)
        })
        .collect();

    Ok(Value::Array(tool_values))
}

/// Get detailed information about a specific tool
/// Usage: tool_info("curl") | tool_info("nmap")
fn bi_tool_info(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let tool_name = args
        .first()
        .and_then(|v| {
            if let Value::Str(s) = v {
                Some(s.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("tool_info requires tool name as argument"))?;

    let db = OSToolsDatabase::new();
    let tool = db
        .get_tool(&tool_name)
        .ok_or_else(|| anyhow!("Tool '{}' not found", tool_name))?;

    let mut rec = BTreeMap::new();
    rec.insert("name".to_string(), Value::Str(tool.name.clone()));
    rec.insert(
        "description".to_string(),
        Value::Str(tool.description.clone()),
    );
    rec.insert("command".to_string(), Value::Str(tool.command.clone()));
    rec.insert(
        "category".to_string(),
        Value::Str(format!("{:?}", tool.category)),
    );
    rec.insert(
        "safety_level".to_string(),
        Value::Str(format!("{:?}", tool.safety_level)),
    );
    rec.insert(
        "requires_admin".to_string(),
        Value::Bool(tool.requires_admin),
    );
    rec.insert(
        "supported_os".to_string(),
        Value::Array(
            tool.supported_os
                .iter()
                .map(|os| Value::Str(format!("{:?}", os)))
                .collect(),
        ),
    );
    rec.insert(
        "common_args".to_string(),
        Value::Array(
            tool.common_args
                .iter()
                .map(|a| Value::Str(a.clone()))
                .collect(),
        ),
    );

    // Add examples
    let examples: Vec<Value> = tool
        .examples
        .iter()
        .map(|ex| {
            let mut ex_rec = BTreeMap::new();
            ex_rec.insert(
                "description".to_string(),
                Value::Str(ex.description.clone()),
            );
            ex_rec.insert("command".to_string(), Value::Str(ex.command.clone()));
            if let Some(ref output) = ex.expected_output {
                ex_rec.insert("expected_output".to_string(), Value::Str(output.clone()));
            }
            Value::Record(ex_rec)
        })
        .collect();
    rec.insert("examples".to_string(), Value::Array(examples));

    // Add parameters if available
    let params: Vec<Value> = tool
        .parameters
        .iter()
        .map(|p| {
            let mut p_rec = BTreeMap::new();
            p_rec.insert("name".to_string(), Value::Str(p.name.clone()));
            p_rec.insert("description".to_string(), Value::Str(p.description.clone()));
            p_rec.insert(
                "type".to_string(),
                Value::Str(format!("{:?}", p.param_type)),
            );
            p_rec.insert("required".to_string(), Value::Bool(p.required));
            if let Some(ref default) = p.default_value {
                p_rec.insert("default".to_string(), Value::Str(default.clone()));
            }
            if !p.enum_values.is_empty() {
                p_rec.insert(
                    "enum_values".to_string(),
                    Value::Array(
                        p.enum_values
                            .iter()
                            .map(|v| Value::Str(v.clone()))
                            .collect(),
                    ),
                );
            }
            Value::Record(p_rec)
        })
        .collect();
    rec.insert("parameters".to_string(), Value::Array(params));

    Ok(Value::Record(rec))
}

/// Get OpenAI-compatible function calling schemas for tools
/// Usage: tool_schema("curl") | tool_schema() for all | tool_schema("network") for category
fn bi_tool_schema(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let db = OSToolsDatabase::new();

    let filter = args.first().and_then(|v| {
        if let Value::Str(s) = v {
            Some(s.clone())
        } else {
            None
        }
    });

    let schemas = if let Some(ref name) = filter {
        // Check if it's a specific tool
        if let Some(tool) = db.get_tool(name) {
            vec![tool.to_openai_function_schema()]
        } else {
            // Check if it's a category
            let cat = match name.to_lowercase().as_str() {
                "network" | "net" => Some(ToolCategory::NetworkTools),
                "web" | "webtools" => Some(ToolCategory::WebTools),
                "cyber" | "cybersecurity" => Some(ToolCategory::CyberSecurity),
                "filesystem" | "file" => Some(ToolCategory::FileSystem),
                "security" | "sec" => Some(ToolCategory::Security),
                "forensics" => Some(ToolCategory::Forensics),
                "crypto" | "cryptography" => Some(ToolCategory::Cryptography),
                _ => None,
            };

            if let Some(c) = cat {
                db.get_category_schemas(&c)
            } else {
                // Search and return schemas for matching tools
                db.search_tools(name)
                    .iter()
                    .map(|t| t.to_openai_function_schema())
                    .collect()
            }
        }
    } else {
        // Return all schemas
        db.to_openai_function_schemas()
    };

    // Convert serde_json::Value to our Value
    let schema_values: Vec<Value> = schemas.into_iter().map(|s| json_to_value(s)).collect();

    Ok(Value::Array(schema_values))
}

/// Execute a tool with given arguments
/// Usage: tool_exec("ls", ["-la"]) | tool_exec("curl", ["-s", "https://example.com"])
fn bi_tool_exec(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!(
            "tool_exec requires: tool_name, [args], [allow_dangerous]"
        ));
    }

    let tool_name = if let Value::Str(s) = &args[0] {
        s.clone()
    } else {
        return Err(anyhow!(
            "tool_exec: first argument must be tool name string"
        ));
    };

    let tool_args: Vec<String> = if args.len() > 1 {
        if let Value::Array(arr) = &args[1] {
            arr.iter()
                .filter_map(|v| {
                    if let Value::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect()
        } else if let Value::Str(s) = &args[1] {
            // Single argument as string
            vec![s.clone()]
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let allow_dangerous = if args.len() > 2 {
        if let Value::Bool(b) = &args[2] {
            *b
        } else {
            false
        }
    } else {
        false
    };

    let db = OSToolsDatabase::new();

    let result = execute_tool_safe(&db, &tool_name, &tool_args, allow_dangerous)
        .map_err(|e| anyhow!("Tool execution failed: {}", e))?;

    // Convert result to Value::Record
    let mut rec = BTreeMap::new();
    rec.insert("success".to_string(), Value::Bool(result.success));
    rec.insert("stdout".to_string(), Value::Str(result.stdout));
    rec.insert("stderr".to_string(), Value::Str(result.stderr));
    rec.insert(
        "exit_code".to_string(),
        result
            .exit_code
            .map(|c| Value::Int(c as i64))
            .unwrap_or(Value::Null),
    );
    rec.insert("tool_name".to_string(), Value::Str(result.tool_name));
    rec.insert(
        "command_executed".to_string(),
        Value::Str(result.command_executed),
    );
    rec.insert(
        "execution_time_ms".to_string(),
        Value::Int(result.execution_time_ms as i64),
    );

    Ok(Value::Record(rec))
}

/// Search for tools by keyword or description
/// Usage: tool_search("network") | tool_search("file copy")
fn bi_tool_search(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let query = args
        .first()
        .and_then(|v| {
            if let Value::Str(s) = v {
                Some(s.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("tool_search requires a search query"))?;

    let db = OSToolsDatabase::new();

    // Use get_recommended_tools for smart search
    let tools = db.get_recommended_tools(&query);

    // If no recommendations, fall back to regular search
    let tools = if tools.is_empty() {
        db.search_tools(&query)
    } else {
        tools
    };

    let tool_values: Vec<Value> = tools
        .iter()
        .map(|t| {
            let mut rec = BTreeMap::new();
            rec.insert("name".to_string(), Value::Str(t.name.clone()));
            rec.insert("description".to_string(), Value::Str(t.description.clone()));
            rec.insert("command".to_string(), Value::Str(t.command.clone()));
            rec.insert(
                "category".to_string(),
                Value::Str(format!("{:?}", t.category)),
            );
            rec.insert(
                "safety".to_string(),
                Value::Str(format!("{:?}", t.safety_level)),
            );
            Value::Record(rec)
        })
        .collect();

    Ok(Value::Array(tool_values))
}

// ===========================================================================
// Recursive Language Model (RLM) Builtins
// ===========================================================================

/// Run a recursive agent that can spawn subagents
/// Usage: rlm_agent("goal", ["tool1", "tool2"])
///        rlm_agent("goal", ["tools"], {max_depth: 3, max_agents: 20})
fn bi_rlm_agent(args: Vec<Value>, _input: Option<Value>, env: &mut Env) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("rlm_agent requires: goal, [tools], [config]"));
    }

    let goal = if let Value::Str(s) = &args[0] {
        s.clone()
    } else {
        return Err(anyhow!("rlm_agent: first argument must be goal string"));
    };

    // Parse tool names
    let tool_names: Vec<String> = if args.len() > 1 {
        if let Value::Array(arr) = &args[1] {
            arr.iter()
                .filter_map(|v| {
                    if let Value::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            vec![
                "ls".to_string(),
                "cat".to_string(),
                "grep".to_string(),
                "find".to_string(),
            ]
        }
    } else {
        vec![
            "ls".to_string(),
            "cat".to_string(),
            "grep".to_string(),
            "find".to_string(),
        ]
    };

    // Parse config if provided
    let config = if args.len() > 2 {
        parse_rlm_config(&args[2])?
    } else {
        RlmConfig::default()
    };

    // Check for model URI
    let model_uri = if args.len() > 3 {
        if let Value::Str(s) = &args[3] {
            Some(s.clone())
        } else {
            None
        }
    } else {
        None
    };

    let tool_refs: Vec<&str> = tool_names.iter().map(|s| s.as_str()).collect();

    let (result, stats) = if let Some(uri) = model_uri {
        run_recursive_with_model(&goal, &tool_refs, &uri, config, false, env)?
    } else {
        run_recursive(&goal, &tool_refs, config, false, env)?
    };

    // Return result with stats
    let mut rec = BTreeMap::new();
    rec.insert("output".to_string(), Value::Str(result));
    rec.insert(
        "total_agents".to_string(),
        Value::Int(stats.total_spawned as i64),
    );
    rec.insert(
        "active_agents".to_string(),
        Value::Int(stats.currently_active as i64),
    );
    rec.insert(
        "messages_sent".to_string(),
        Value::Int(stats.messages_sent as i64),
    );
    rec.insert(
        "elapsed_ms".to_string(),
        Value::Int(stats.elapsed_ms as i64),
    );

    Ok(Value::Record(rec))
}

/// Create an RLM configuration
/// Usage: rlm_config({max_depth: 5, max_agents: 50, timeout: 60})
fn bi_rlm_config(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let config = if args.is_empty() {
        RlmConfig::default()
    } else {
        parse_rlm_config(&args[0])?
    };

    let mut rec = BTreeMap::new();
    rec.insert("max_depth".to_string(), Value::Int(config.max_depth as i64));
    rec.insert(
        "max_agents".to_string(),
        Value::Int(config.max_agents as i64),
    );
    rec.insert(
        "agent_timeout_secs".to_string(),
        Value::Int(config.agent_timeout_secs as i64),
    );
    rec.insert(
        "max_concurrent_children".to_string(),
        Value::Int(config.max_concurrent_children as i64),
    );
    rec.insert(
        "trace_enabled".to_string(),
        Value::Bool(config.trace_enabled),
    );
    if let Some(uri) = config.subagent_model_uri {
        rec.insert("subagent_model_uri".to_string(), Value::Str(uri));
    }

    Ok(Value::Record(rec))
}

/// Get RLM statistics from the last run
/// Usage: rlm_stats()
fn bi_rlm_stats(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    // If input is a record with stats, format it nicely
    let stats_value = input.or_else(|| args.first().cloned());

    if let Some(Value::Record(rec)) = stats_value {
        // Already a stats record, return as-is
        return Ok(Value::Record(rec));
    }

    // Return default stats structure
    let mut rec = BTreeMap::new();
    rec.insert("total_spawned".to_string(), Value::Int(0));
    rec.insert("currently_active".to_string(), Value::Int(0));
    rec.insert("messages_sent".to_string(), Value::Int(0));
    rec.insert("elapsed_ms".to_string(), Value::Int(0));
    rec.insert("max_depth".to_string(), Value::Int(5));
    rec.insert("max_agents".to_string(), Value::Int(50));

    Ok(Value::Record(rec))
}

/// Spawn a subagent (used in agent context)
/// Usage: rlm_spawn("name", "goal", ["tools"])
fn bi_rlm_spawn(args: Vec<Value>, _input: Option<Value>, env: &mut Env) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("rlm_spawn requires: name, goal, [tools]"));
    }

    let name = if let Value::Str(s) = &args[0] {
        s.clone()
    } else {
        return Err(anyhow!("rlm_spawn: name must be a string"));
    };

    let goal = if let Value::Str(s) = &args[1] {
        s.clone()
    } else {
        return Err(anyhow!("rlm_spawn: goal must be a string"));
    };

    let tool_names: Vec<String> = if args.len() > 2 {
        if let Value::Array(arr) = &args[2] {
            arr.iter()
                .filter_map(|v| {
                    if let Value::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            vec!["ls".to_string(), "cat".to_string()]
        }
    } else {
        vec!["ls".to_string(), "cat".to_string()]
    };

    // Run a single-depth recursive agent for the spawn
    let config = RlmConfig {
        max_depth: 1, // Single level only
        max_agents: 5,
        agent_timeout_secs: 30,
        ..Default::default()
    };

    let tool_refs: Vec<&str> = tool_names.iter().map(|s| s.as_str()).collect();
    let (result, stats) = run_recursive(&goal, &tool_refs, config, false, env)?;

    let mut rec = BTreeMap::new();
    rec.insert("name".to_string(), Value::Str(name));
    rec.insert("goal".to_string(), Value::Str(goal));
    rec.insert("output".to_string(), Value::Str(result));
    rec.insert("success".to_string(), Value::Bool(stats.total_spawned > 0));
    rec.insert(
        "agents_used".to_string(),
        Value::Int(stats.total_spawned as i64),
    );
    rec.insert(
        "elapsed_ms".to_string(),
        Value::Int(stats.elapsed_ms as i64),
    );

    Ok(Value::Record(rec))
}

/// Parse RLM configuration from a Value
fn parse_rlm_config(value: &Value) -> Result<RlmConfig> {
    let mut config = RlmConfig::default();

    if let Value::Record(rec) = value {
        if let Some(Value::Int(n)) = rec.get("max_depth") {
            config.max_depth = *n as usize;
        }
        if let Some(Value::Int(n)) = rec.get("max_agents") {
            config.max_agents = *n as usize;
        }
        if let Some(Value::Int(n)) = rec.get("timeout") {
            config.agent_timeout_secs = *n as u64;
        }
        if let Some(Value::Int(n)) = rec.get("agent_timeout_secs") {
            config.agent_timeout_secs = *n as u64;
        }
        if let Some(Value::Int(n)) = rec.get("max_concurrent_children") {
            config.max_concurrent_children = *n as usize;
        }
        if let Some(Value::Bool(b)) = rec.get("trace_enabled") {
            config.trace_enabled = *b;
        }
        if let Some(Value::Bool(b)) = rec.get("trace") {
            config.trace_enabled = *b;
        }
        if let Some(Value::Str(s)) = rec.get("model_uri") {
            config.subagent_model_uri = Some(s.clone());
        }
        if let Some(Value::Str(s)) = rec.get("subagent_model_uri") {
            config.subagent_model_uri = Some(s.clone());
        }
    }

    Ok(config)
}

// =============== New Aspirational Feature Builtins ===============

/// sh(args) - Execute a shell command directly
/// Usage: sh(["echo", "hello"]) or sh("echo hello")
fn bi_sh(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    use std::process::Command;

    if args.is_empty() {
        return Err(anyhow!("sh requires command arguments"));
    }

    let (program, cmd_args): (String, Vec<String>) = match &args[0] {
        Value::Str(s) => {
            // Split string into program and args
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.is_empty() {
                return Err(anyhow!("sh: empty command"));
            }
            (
                parts[0].to_string(),
                parts[1..].iter().map(|s| s.to_string()).collect(),
            )
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                return Err(anyhow!("sh: empty command array"));
            }
            let program = match &arr[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(anyhow!("sh: first argument must be program name string")),
            };
            let args: Result<Vec<String>> = arr[1..]
                .iter()
                .map(|v| match v {
                    Value::Str(s) => Ok(s.clone()),
                    Value::Int(n) => Ok(n.to_string()),
                    Value::Float(f) => Ok(f.to_string()),
                    _ => Err(anyhow!("sh: arguments must be strings or numbers")),
                })
                .collect();
            (program, args?)
        }
        _ => return Err(anyhow!("sh requires string or array of strings")),
    };

    let output = Command::new(&program)
        .args(&cmd_args)
        .output()
        .with_context(|| format!("sh: failed to execute '{}'", program))?;

    let mut record = BTreeMap::new();
    record.insert(
        "stdout".to_string(),
        Value::Str(String::from_utf8_lossy(&output.stdout).to_string()),
    );
    record.insert(
        "stderr".to_string(),
        Value::Str(String::from_utf8_lossy(&output.stderr).to_string()),
    );
    record.insert("success".to_string(), Value::Bool(output.status.success()));
    record.insert(
        "exit_code".to_string(),
        Value::Int(output.status.code().unwrap_or(-1) as i64),
    );

    Ok(Value::Record(record))
}

/// now() - Get current Unix timestamp in seconds
fn bi_now(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| anyhow!("now: system time error: {}", e))?;
    Ok(Value::Int(duration.as_secs() as i64))
}

/// sort_by(fn, direction?) - Sort array by key function
/// Usage: arr | sort_by(fn(x) => x.name) or arr | sort_by(fn(x) => x.size, "desc")
fn bi_sort_by(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let arr = input
        .as_ref()
        .and_then(|v| match v {
            Value::Array(a) => Some(a.as_slice()),
            _ => None,
        })
        .or_else(|| {
            args.first().and_then(|v| match v {
                Value::Array(a) => Some(a.as_slice()),
                _ => None,
            })
        })
        .ok_or_else(|| anyhow!("sort_by requires array input"))?;

    let lambda = args
        .iter()
        .find_map(|v| match v {
            Value::Lambda(l) => Some(l),
            _ => None,
        })
        .ok_or_else(|| anyhow!("sort_by requires a lambda function"))?;

    let descending = args.iter().any(|v| match v {
        Value::Str(s) => s == "desc" || s == "descending",
        _ => false,
    });

    // Extract keys for each element
    let mut keyed: Vec<(Value, Value)> = arr
        .iter()
        .map(|item| {
            let key = call_lambda(lambda, &[item.clone()], env).unwrap_or(Value::Null);
            (key, item.clone())
        })
        .collect();

    // Sort by keys
    keyed.sort_by(|(a, _), (b, _)| {
        let cmp = sort_compare_values(a, b);
        if descending {
            cmp.reverse()
        } else {
            cmp
        }
    });

    let result: Vec<Value> = keyed.into_iter().map(|(_, v)| v).collect();
    Ok(Value::Array(result))
}

/// Helper to compare two values for sorting
fn sort_compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Int(x), Value::Float(y)) => (*x as f64).partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Float(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)).unwrap_or(Ordering::Equal),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => Ordering::Equal,
    }
}

/// save_json(path, value?) - Save value as JSON to file
/// Usage: data | save_json("output.json") or save_json("output.json", data)
fn bi_save_json(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("save_json requires a file path"));
    }

    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(anyhow!("save_json: path must be a string")),
    };

    let value = if args.len() > 1 {
        args[1].clone()
    } else {
        input.ok_or_else(|| anyhow!("save_json requires a value to save"))?
    };

    // Convert Value to JSON using existing function
    let json = value_to_json(value);
    let json_str = serde_json::to_string_pretty(&json)
        .map_err(|e| anyhow!("save_json: JSON serialization failed: {}", e))?;

    // Write to file
    fs::write(&path, &json_str)
        .with_context(|| format!("save_json: failed to write to '{}'", path))?;

    let mut record = BTreeMap::new();
    record.insert("path".to_string(), Value::Str(path));
    record.insert("bytes".to_string(), Value::Int(json_str.len() as i64));
    record.insert("success".to_string(), Value::Bool(true));

    Ok(Value::Record(record))
}

/// mcp_server_start(config) - Start a custom MCP server
/// Usage: mcp_server_start({name: "fs", type: "builtin", config: {...}})
fn bi_mcp_server_start(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("mcp_server_start requires a configuration record"));
    }

    let config = match &args[0] {
        Value::Record(r) => r,
        _ => return Err(anyhow!("mcp_server_start: config must be a record")),
    };

    let name = config
        .get("name")
        .and_then(|v| match v {
            Value::Str(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "default".to_string());

    let server_type = config
        .get("type")
        .and_then(|v| match v {
            Value::Str(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "builtin".to_string());

    // Generate a unique endpoint
    let port = 3000
        + (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u16 % 1000)
            .unwrap_or(0));
    let endpoint = format!("http://localhost:{}", port);

    // Create server info record
    let mut result = BTreeMap::new();
    result.insert("name".to_string(), Value::Str(name));
    result.insert("type".to_string(), Value::Str(server_type));
    result.insert("endpoint".to_string(), Value::Str(endpoint));
    result.insert("port".to_string(), Value::Int(port as i64));
    result.insert("status".to_string(), Value::Str("started".to_string()));

    // Include original config
    if let Some(inner_config) = config.get("config") {
        result.insert("config".to_string(), inner_config.clone());
    }

    Ok(Value::Record(result))
}

/// agent_with_mcp(goal, tools, endpoint) - Create agent with MCP tool access
/// Usage: agent_with_mcp("Task description", ["mcp:read_file", "mcp:list_dir"], server.endpoint)
fn bi_agent_with_mcp(args: Vec<Value>, _input: Option<Value>, env: &mut Env) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("agent_with_mcp requires goal and tools array"));
    }

    let goal = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(anyhow!("agent_with_mcp: goal must be a string")),
    };

    let tools: Vec<String> = match &args[1] {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| match v {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => return Err(anyhow!("agent_with_mcp: tools must be an array of strings")),
    };

    let endpoint = if args.len() > 2 {
        match &args[2] {
            Value::Str(s) => Some(s.clone()),
            Value::Array(endpoints) => {
                // Multiple endpoints - join them
                let eps: Vec<String> = endpoints
                    .iter()
                    .filter_map(|v| match v {
                        Value::Str(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect();
                Some(eps.join(","))
            }
            _ => None,
        }
    } else {
        None
    };

    // Create agent wrapper with MCP capabilities
    let mut agent_record = BTreeMap::new();
    agent_record.insert("goal".to_string(), Value::Str(goal.clone()));
    agent_record.insert(
        "tools".to_string(),
        Value::Array(tools.iter().map(|s| Value::Str(s.clone())).collect()),
    );
    agent_record.insert("type".to_string(), Value::Str("mcp_agent".to_string()));

    if let Some(ep) = &endpoint {
        agent_record.insert("endpoint".to_string(), Value::Str(ep.clone()));
    }

    // Extract base tool names (remove mcp: prefix)
    let base_tools: Vec<&str> = tools
        .iter()
        .map(|s| s.strip_prefix("mcp:").unwrap_or(s.as_str()))
        .collect();

    // Run the agent with the goal
    let dry_run = false;
    let max_steps = 10;
    let result = crate::ai::agents::swarm::run_sync(&goal, &base_tools, max_steps, dry_run, env)?;

    agent_record.insert("result".to_string(), Value::Str(result));
    agent_record.insert("status".to_string(), Value::Str("completed".to_string()));

    Ok(Value::Record(agent_record))
}

/// each(fn) - Like map but for side effects, returns original array
/// Usage: arr | each(fn(x) => print(x))
fn bi_each(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let arr = input
        .as_ref()
        .and_then(|v| match v {
            Value::Array(a) => Some(a.clone()),
            _ => None,
        })
        .or_else(|| {
            args.first().and_then(|v| match v {
                Value::Array(a) => Some(a.clone()),
                _ => None,
            })
        })
        .ok_or_else(|| anyhow!("each requires array input"))?;

    let lambda = args
        .iter()
        .find_map(|v| match v {
            Value::Lambda(l) => Some(l),
            _ => None,
        })
        .ok_or_else(|| anyhow!("each requires a lambda function"))?;

    // Apply lambda to each element for side effects
    for item in &arr {
        let _ = call_lambda(lambda, &[item.clone()], env);
    }

    // Return original array
    Ok(Value::Array(arr))
}

/// in(value, collection) - Check if value is in collection
/// Usage: "x" | in(["a", "b", "x"]) or in("x", ["a", "b", "x"])
fn bi_in(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (needle, haystack) = if let Some(ref inp) = input {
        if args.is_empty() {
            return Err(anyhow!("in requires a collection to search"));
        }
        (inp, &args[0])
    } else {
        if args.len() < 2 {
            return Err(anyhow!("in requires value and collection"));
        }
        (&args[0], &args[1])
    };

    let found = match haystack {
        Value::Array(arr) => arr.iter().any(|v| values_equal(needle, v)),
        Value::Str(s) => {
            if let Value::Str(n) = needle {
                s.contains(n.as_str())
            } else {
                false
            }
        }
        Value::Record(rec) => {
            if let Value::Str(key) = needle {
                rec.contains_key(key)
            } else {
                false
            }
        }
        _ => false,
    };

    Ok(Value::Bool(found))
}

/// Check if two values are equal
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => (x - y).abs() < f64::EPSILON,
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => {
            (*x as f64 - y).abs() < f64::EPSILON
        }
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| values_equal(a, b))
        }
        _ => false,
    }
}
