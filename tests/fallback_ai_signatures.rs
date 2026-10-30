//! The fallback `ai_*` and `vault_*` builtins: argument and piped-input
//! refusals for every spelling, and a regression test for each defect found
//! while declaring them.
//!
//! Every call here is refused before its body reaches a network service, a
//! GPU, a model file or the IronVault CLI, so the whole file runs on Windows
//! and Linux without network access. The one exception is the `nosuch:`
//! handles, which name no backend and so never leave the process.

use iron_shell::builtins::call_with_input;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::collections::BTreeMap;

fn env() -> Env {
    static ISOLATED: std::sync::Once = std::sync::Once::new();
    ISOLATED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_fallback_ai_{}", std::process::id()));
        std::env::set_var("IRONSHELL_SESSION_DIR", dir);
    });
    Env::new()
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn rec(pairs: Vec<(&str, Value)>) -> Value {
    let map: BTreeMap<String, Value> = pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    Value::Record(map)
}

/// The refusal a call produces, or a panic naming the call that was answered.
fn refusal(name: &str, args: Vec<Value>, input: Option<Value>) -> String {
    match call_with_input(name, args.clone(), input, &mut env()) {
        Ok(v) => panic!("{name}({args:?}) was accepted and answered {v:?}"),
        Err(e) => e.to_string(),
    }
}

/// Refused with the given coded error.
fn refused_with(code: &str, name: &str, args: Vec<Value>, input: Option<Value>) {
    let message = refusal(name, args, input);
    assert!(
        message.contains(code),
        "{name} should be refused as {code}, got: {message}"
    );
}

fn bad_arg(name: &str, args: Vec<Value>) {
    refused_with("E_BAD_ARG", name, args, None);
}

/// Every spelling a caller can use: the canonical name, the dash form, and
/// the one alias the table names (`irongate`).
const SPELLINGS: &[&str] = &[
    "ai_add_route",
    "ai-add-route",
    "ai_backends",
    "ai-backends",
    "ai_convert_model",
    "ai-convert-model",
    "ai_detect",
    "ai-detect",
    "ai_gateway",
    "ai-gateway",
    "irongate",
    "ai_gpu_memory",
    "ai-gpu-memory",
    "ai_local_embed",
    "ai-local-embed",
    "ai_local_generate",
    "ai-local-generate",
    "ai_local_load",
    "ai-local-load",
    "ai_local_unload",
    "ai-local-unload",
    "ai_model_fits",
    "ai-model-fits",
    "ai_reset_usage",
    "ai-reset-usage",
    "ai_set_default",
    "ai-set-default",
    "ai_set_load_balancing",
    "ai-set-load-balancing",
    "ai_supported_conversions",
    "ai-supported-conversions",
    "vault_conversions",
    "vault-conversions",
    "vault_convert",
    "vault-convert",
    "vault_models",
    "vault-models",
];

/// The canonical (underscore) name a spelling reaches.
fn canonical(spelling: &str) -> &'static str {
    match spelling {
        "irongate" | "ai-gateway" | "ai_gateway" => "ai_gateway",
        "ai_add_route" | "ai-add-route" => "ai_add_route",
        "ai_backends" | "ai-backends" => "ai_backends",
        "ai_convert_model" | "ai-convert-model" => "ai_convert_model",
        "ai_detect" | "ai-detect" => "ai_detect",
        "ai_gpu_memory" | "ai-gpu-memory" => "ai_gpu_memory",
        "ai_local_embed" | "ai-local-embed" => "ai_local_embed",
        "ai_local_generate" | "ai-local-generate" => "ai_local_generate",
        "ai_local_load" | "ai-local-load" => "ai_local_load",
        "ai_local_unload" | "ai-local-unload" => "ai_local_unload",
        "ai_model_fits" | "ai-model-fits" => "ai_model_fits",
        "ai_reset_usage" | "ai-reset-usage" => "ai_reset_usage",
        "ai_set_default" | "ai-set-default" => "ai_set_default",
        "ai_set_load_balancing" | "ai-set-load-balancing" => "ai_set_load_balancing",
        "ai_supported_conversions" | "ai-supported-conversions" => "ai_supported_conversions",
        "vault_conversions" | "vault-conversions" => "vault_conversions",
        "vault_convert" | "vault-convert" => "vault_convert",
        "vault_models" | "vault-models" => "vault_models",
        other => panic!("no canonical name for {other}"),
    }
}

/// A full, correctly typed argument list for a canonical name. A surplus
/// argument is appended to this in the tests below.
fn full_args(canonical: &str) -> Vec<Value> {
    match canonical {
        "ai_add_route" => vec![rec(vec![("name", s("r")), ("condition", s("always"))])],
        "ai_convert_model" | "vault_convert" => {
            vec![rec(vec![("name", s("m")), ("to_format", s("gguf"))])]
        }
        "ai_model_fits" => vec![rec(vec![("params_b", Value::Float(7.0))])],
        "ai_set_default" => vec![s("openai")],
        "ai_set_load_balancing" => vec![s("round_robin")],
        "ai_local_embed" => vec![s("candle:x"), Value::Array(vec![s("text")])],
        "ai_local_generate" => vec![
            s("candle:x"),
            s("hi"),
            rec(vec![("max_tokens", Value::Int(4))]),
        ],
        "ai_local_load" => vec![s("model.gguf")],
        "ai_local_unload" => vec![s("candle:x")],
        _ => vec![],
    }
}

/// Every name in the table accepts its full, valid argument list up to the
/// declared arity, so the surplus and piped-input tests start from a call the
/// shape check would otherwise pass.
#[test]
fn every_spelling_refuses_a_surplus_argument() {
    for spelling in SPELLINGS {
        let mut args = full_args(canonical(spelling));
        args.push(Value::Int(1));
        bad_arg(spelling, args);
    }
}

#[test]
fn every_spelling_refuses_a_piped_subject() {
    for spelling in SPELLINGS {
        let args = full_args(canonical(spelling));
        refused_with("E_BAD_ARG", spelling, args, Some(s("x")));
    }
}

#[test]
fn every_spelling_refuses_a_missing_required_argument() {
    for spelling in SPELLINGS {
        let required = match canonical(spelling) {
            "ai_add_route"
            | "ai_convert_model"
            | "vault_convert"
            | "ai_model_fits"
            | "ai_set_default"
            | "ai_set_load_balancing"
            | "ai_local_load"
            | "ai_local_unload" => 1,
            "ai_local_embed" => 2,
            "ai_local_generate" => 2,
            _ => 0,
        };
        if required == 0 {
            continue;
        }
        let mut args = full_args(canonical(spelling));
        args.truncate(required - 1);
        bad_arg(spelling, args);
    }
}

#[test]
fn every_spelling_refuses_a_wrongly_typed_first_argument() {
    for spelling in SPELLINGS {
        let needs_arg = !full_args(canonical(spelling)).is_empty();
        if needs_arg {
            // An Int is no config record, no provider name, no handle, no path
            // and no strategy: every first argument in this table is refused.
            bad_arg(spelling, vec![Value::Int(7)]);
        }
    }
}

// ---- argument refusals -----------------------------------------------------

#[test]
fn ai_add_route_refuses_a_misspelt_key_instead_of_defaulting() {
    bad_arg(
        "ai_add_route",
        vec![rec(vec![
            ("name", s("r")),
            ("condition", s("always")),
            ("prority", Value::Int(1)),
        ])],
    );
}

#[test]
fn ai_add_route_refuses_a_mistyped_priority_instead_of_defaulting() {
    bad_arg(
        "ai_add_route",
        vec![rec(vec![
            ("name", s("r")),
            ("condition", s("always")),
            ("priority", s("100")),
        ])],
    );
}

#[test]
fn ai_add_route_refuses_a_negative_priority_instead_of_wrapping() {
    bad_arg(
        "ai_add_route",
        vec![rec(vec![
            ("name", s("r")),
            ("condition", s("always")),
            ("priority", Value::Int(-1)),
        ])],
    );
}

#[test]
fn ai_add_route_refuses_a_mistyped_allow_fallback_instead_of_defaulting() {
    bad_arg(
        "ai_add_route",
        vec![rec(vec![
            ("name", s("r")),
            ("condition", s("always")),
            ("allow_fallback", s("no")),
        ])],
    );
}

#[test]
fn ai_add_route_refuses_a_non_string_target_instead_of_dropping_it() {
    bad_arg(
        "ai_add_route",
        vec![rec(vec![
            ("name", s("r")),
            ("condition", s("always")),
            ("target", Value::Int(5)),
        ])],
    );
}

#[test]
fn ai_add_route_refuses_an_unknown_condition_with_a_coded_error() {
    // This was an uncoded anyhow error before the declaration.
    refused_with(
        "E_BAD_ARG",
        "ai_add_route",
        vec![rec(vec![("name", s("r")), ("condition", s("bogus"))])],
        None,
    );
}

#[test]
fn ai_add_route_refuses_an_unknown_target_with_a_coded_error() {
    refused_with(
        "E_BAD_ARG",
        "ai_add_route",
        vec![rec(vec![
            ("name", s("r")),
            ("condition", s("always")),
            ("target", s("no-such-provider")),
        ])],
        None,
    );
}

#[test]
fn ai_set_default_refuses_an_unknown_provider_with_a_coded_error() {
    refused_with(
        "E_BAD_ARG",
        "ai_set_default",
        vec![s("no-such-provider")],
        None,
    );
}

#[test]
fn ai_set_load_balancing_refuses_an_unknown_strategy_with_a_coded_error() {
    refused_with("E_BAD_ARG", "ai_set_load_balancing", vec![s("bogus")], None);
}

#[test]
fn ai_set_load_balancing_refuses_a_negative_weight_instead_of_wrapping() {
    bad_arg(
        "ai_set_load_balancing",
        vec![rec(vec![
            ("type", s("weighted")),
            ("weights", rec(vec![("openai", Value::Int(-1))])),
        ])],
    );
}

#[test]
fn ai_set_load_balancing_refuses_a_non_numeric_weight_instead_of_weight_one() {
    bad_arg(
        "ai_set_load_balancing",
        vec![rec(vec![
            ("type", s("weighted")),
            ("weights", rec(vec![("openai", s("3"))])),
        ])],
    );
}

#[test]
fn ai_set_load_balancing_refuses_a_fractional_weight() {
    bad_arg(
        "ai_set_load_balancing",
        vec![rec(vec![
            ("type", s("weighted")),
            ("weights", rec(vec![("openai", Value::Float(1.5))])),
        ])],
    );
}

#[test]
fn ai_set_load_balancing_refuses_an_unknown_weight_provider_with_a_coded_error() {
    bad_arg(
        "ai_set_load_balancing",
        vec![rec(vec![
            ("type", s("weighted")),
            ("weights", rec(vec![("nosuch", Value::Int(1))])),
        ])],
    );
}

#[test]
fn ai_set_load_balancing_refuses_an_unknown_record_key() {
    bad_arg(
        "ai_set_load_balancing",
        vec![rec(vec![
            ("type", s("weighted")),
            ("weights", rec(vec![("openai", Value::Int(1))])),
            ("extra", Value::Int(1)),
        ])],
    );
}

#[test]
fn ai_set_load_balancing_refuses_an_empty_weights_record() {
    bad_arg(
        "ai_set_load_balancing",
        vec![rec(vec![("type", s("weighted")), ("weights", rec(vec![]))])],
    );
}

#[test]
fn ai_model_fits_refuses_a_call_with_no_size() {
    bad_arg(
        "ai_model_fits",
        vec![rec(vec![("quantization", s("q4_k_m"))])],
    );
}

#[test]
fn ai_model_fits_refuses_both_sizes() {
    bad_arg(
        "ai_model_fits",
        vec![rec(vec![
            ("params_b", Value::Float(7.0)),
            ("file_size_mb", Value::Int(4096)),
        ])],
    );
}

#[test]
fn ai_model_fits_refuses_a_negative_file_size_instead_of_wrapping() {
    // A negative Int was cast to u64 and overflowed the estimate.
    bad_arg(
        "ai_model_fits",
        vec![rec(vec![("file_size_mb", Value::Int(-1))])],
    );
}

#[test]
fn ai_model_fits_refuses_an_absurd_parameter_count_instead_of_overflowing() {
    // A huge float saturated the u64 cast and then overflowed the +20% add.
    bad_arg(
        "ai_model_fits",
        vec![rec(vec![("params_b", Value::Float(1e300))])],
    );
}

#[test]
fn ai_model_fits_refuses_a_zero_parameter_count() {
    bad_arg(
        "ai_model_fits",
        vec![rec(vec![("params_b", Value::Float(0.0))])],
    );
}

#[test]
fn ai_model_fits_refuses_an_unknown_key() {
    bad_arg(
        "ai_model_fits",
        vec![rec(vec![
            ("params_b", Value::Float(7.0)),
            ("paramz", Value::Float(1.0)),
        ])],
    );
}

#[test]
fn ai_local_generate_refuses_an_unknown_params_key() {
    bad_arg(
        "ai_local_generate",
        vec![
            s("candle:x"),
            s("hi"),
            rec(vec![("max_tokenz", Value::Int(5))]),
        ],
    );
}

#[test]
fn ai_local_generate_refuses_a_float_max_tokens_instead_of_ignoring_it() {
    bad_arg(
        "ai_local_generate",
        vec![
            s("candle:x"),
            s("hi"),
            rec(vec![("max_tokens", Value::Float(5.0))]),
        ],
    );
}

#[test]
fn ai_local_generate_refuses_zero_max_tokens() {
    bad_arg(
        "ai_local_generate",
        vec![
            s("candle:x"),
            s("hi"),
            rec(vec![("max_tokens", Value::Int(0))]),
        ],
    );
}

#[test]
fn ai_local_generate_refuses_a_negative_seed_instead_of_wrapping() {
    bad_arg(
        "ai_local_generate",
        vec![s("candle:x"), s("hi"), rec(vec![("seed", Value::Int(-1))])],
    );
}

#[test]
fn ai_local_generate_refuses_a_string_temperature() {
    bad_arg(
        "ai_local_generate",
        vec![s("candle:x"), s("hi"), rec(vec![("temperature", s("hot"))])],
    );
}

#[test]
fn ai_local_generate_refuses_a_non_record_params_argument() {
    bad_arg(
        "ai_local_generate",
        vec![s("candle:x"), s("hi"), Value::Int(5)],
    );
}

#[test]
fn ai_local_embed_refuses_a_non_string_element_instead_of_embedding_its_text() {
    bad_arg(
        "ai_local_embed",
        vec![s("candle:x"), Value::Array(vec![Value::Int(1)])],
    );
}

#[test]
fn ai_local_embed_refuses_an_empty_array() {
    bad_arg("ai_local_embed", vec![s("candle:x"), Value::Array(vec![])]);
}

#[test]
fn ai_local_unload_names_an_unknown_handle_with_a_coded_error() {
    // No compiled backend is called "nosuch", so this never reaches one.
    refused_with(
        "E_NOT_FOUND",
        "ai_local_unload",
        vec![s("nosuch:model")],
        None,
    );
}

#[test]
fn ai_local_embed_names_an_unknown_handle_with_a_coded_error() {
    refused_with(
        "E_NOT_FOUND",
        "ai_local_embed",
        vec![s("nosuch:model"), Value::Array(vec![s("text")])],
        None,
    );
}

#[test]
fn ai_convert_model_refuses_the_legacy_path_form_with_a_coded_error() {
    // This was an uncoded anyhow error before the declaration.
    bad_arg(
        "ai_convert_model",
        vec![rec(vec![
            ("source", s("a.gguf")),
            ("target", s("b.safetensors")),
        ])],
    );
}

#[test]
fn vault_convert_refuses_an_unknown_key_instead_of_ignoring_it() {
    bad_arg(
        "vault_convert",
        vec![rec(vec![
            ("name", s("m")),
            ("to_format", s("gguf")),
            ("outptu", s("x.gguf")),
        ])],
    );
}

#[test]
fn vault_convert_refuses_a_non_boolean_validate_instead_of_reading_false() {
    bad_arg(
        "vault_convert",
        vec![rec(vec![
            ("name", s("m")),
            ("to_format", s("gguf")),
            ("validate", s("yes")),
        ])],
    );
}

#[test]
fn vault_convert_refuses_a_non_string_quantization_instead_of_dropping_it() {
    bad_arg(
        "vault_convert",
        vec![rec(vec![
            ("name", s("m")),
            ("to_format", s("gguf")),
            ("quantization", Value::Int(4)),
        ])],
    );
}

#[test]
fn vault_convert_refuses_a_non_string_output_instead_of_dropping_it() {
    bad_arg(
        "vault_convert",
        vec![rec(vec![
            ("name", s("m")),
            ("to_format", s("gguf")),
            ("output", Value::Int(1)),
        ])],
    );
}

#[test]
fn vault_convert_refuses_an_option_like_name_before_iv_runs() {
    bad_arg(
        "vault_convert",
        vec![rec(vec![("name", s("-x")), ("to_format", s("gguf"))])],
    );
}

#[test]
fn vault_convert_refuses_a_missing_target_format() {
    bad_arg("vault_convert", vec![rec(vec![("name", s("m"))])]);
}

#[test]
fn vault_convert_refuses_a_missing_name() {
    bad_arg("vault_convert", vec![rec(vec![("to_format", s("gguf"))])]);
}

#[test]
fn ai_detect_refuses_a_piped_subject_instead_of_discarding_it() {
    refused_with("E_BAD_ARG", "ai_detect", vec![], Some(s("x")));
}

#[test]
fn ai_reset_usage_refuses_a_piped_subject_before_resetting() {
    refused_with("E_BAD_ARG", "ai_reset_usage", vec![], Some(s("x")));
}

#[test]
fn vault_models_refuses_a_piped_subject_before_reading_the_vault() {
    refused_with("E_BAD_ARG", "vault_models", vec![], Some(s("x")));
}
