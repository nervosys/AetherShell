use aethershell::{
    builtins::call, env::Env, eval::eval_program, parser::parse_program, value::Value,
};

struct FileFixture {
    root: std::path::PathBuf,
    directory: std::path::PathBuf,
}

impl FileFixture {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap();
        let directory = root.join(format!("ae_signature_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        Self { root, directory }
    }
    fn path(&self, name: &str) -> Value {
        Value::Str(self.directory.join(name).to_string_lossy().into_owned())
    }
}

impl Drop for FileFixture {
    fn drop(&mut self) {
        assert_eq!(self.directory.parent(), Some(self.root.as_path()));
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn concurrent_file_writes_do_not_share_a_temporary_file() {
    let fixture = FileFixture::new();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    std::thread::scope(|scope| {
        for index in 0..8 {
            let path = fixture.path(&format!("writer-{index}"));
            let barrier = barrier.clone();
            scope.spawn(move || {
                let mut env = Env::new();
                let text = format!("writer-{index}:").repeat(8192);
                barrier.wait();
                for _ in 0..12 {
                    call(
                        "file_write",
                        vec![path.clone(), Value::Str(text.clone())],
                        &mut env,
                    )
                    .unwrap();
                    let Value::Str(filename) = &path else {
                        unreachable!()
                    };
                    assert_eq!(std::fs::read_to_string(filename).unwrap(), text);
                }
            });
        }
    });
    assert_eq!(std::fs::read_dir(&fixture.directory).unwrap().count(), 8);
}

#[test]
fn failed_atomic_write_is_coded_and_removes_its_temporary_file() {
    let fixture = FileFixture::new();
    let destination = fixture.directory.join("directory");
    std::fs::create_dir(&destination).unwrap();
    let error = call(
        "file_write",
        vec![fixture.path("directory"), Value::Str("x".into())],
        &mut Env::new(),
    )
    .unwrap_err();
    let structured: serde_json::Value = serde_json::from_str(&error.to_string()).unwrap();
    assert!(
        matches!(
            structured["error"]["code"].as_str(),
            Some("E_IO" | "E_POLICY_DENY")
        ),
        "{error}"
    );
    assert_eq!(std::fs::read_dir(&fixture.directory).unwrap().count(), 1);
}

#[test]
fn malformed_patches_are_refused_before_any_file_is_changed() {
    let fixture = FileFixture::new();
    std::fs::write(fixture.directory.join("x"), "hello").unwrap();
    let patch = Value::Record(std::collections::BTreeMap::from([
        ("find".into(), Value::Str("hello".into())),
        ("replace".into(), Value::Str("changed".into())),
    ]));
    let error = call(
        "file_patch",
        vec![
            fixture.path("x"),
            Value::Array(vec![patch, Value::Bool(false)]),
        ],
        &mut Env::new(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    assert_eq!(
        std::fs::read_to_string(fixture.directory.join("x")).unwrap(),
        "hello"
    );
}

#[test]
fn missing_unicode_patch_text_does_not_panic_or_corrupt_the_file() {
    let fixture = FileFixture::new();
    std::fs::write(fixture.directory.join("x"), "hello").unwrap();
    let result = call(
        "file_replace",
        vec![
            fixture.path("x"),
            Value::Str("界".repeat(50)),
            Value::Str("changed".into()),
        ],
        &mut Env::new(),
    )
    .unwrap();
    let Value::Record(result) = result else {
        panic!("expected patch report")
    };
    assert_eq!(result["success"], Value::Bool(false));
    assert_eq!(
        std::fs::read_to_string(fixture.directory.join("x")).unwrap(),
        "hello"
    );
}

#[test]
fn telemetry_filters_refuse_wrong_types_and_unsupported_fields() {
    for (key, value) in [
        ("category", Value::Int(1)),
        ("since", Value::Str("yesterday".into())),
    ] {
        let options = Value::Record(std::collections::BTreeMap::from([(key.into(), value)]));
        let error = call("telemetry_report", vec![options], &mut Env::new()).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn metadata_options_do_not_silently_turn_wrong_shapes_into_defaults() {
    let cases = [
        ("tool_schema", Value::Uri("https://example.com".into())),
        (
            "telemetry_report",
            Value::Table(aethershell::value::Table {
                rows: vec![],
                schema: vec![],
            }),
        ),
    ];
    for (name, value) in cases {
        let error = call(name, vec![value], &mut Env::new()).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn random_generators_honor_lengths_and_validate_character_sets() {
    let mut env = Env::new();
    assert_eq!(
        call("crypto_random_bytes", vec![Value::Int(0)], &mut env).unwrap(),
        Value::Str(String::new())
    );
    assert_eq!(
        call(
            "crypto_random_string",
            vec![Value::Int(300), Value::Str("界".into())],
            &mut env
        )
        .unwrap(),
        Value::Str("界".repeat(300))
    );
    // More than 256 distinct characters previously caused the Windows rejection
    // sampler to loop forever and Unix to return an empty string.
    let charset: String = (0x400..0x600).filter_map(char::from_u32).collect();
    let Value::Str(text) = call(
        "crypto_random_string",
        vec![Value::Int(300), Value::Str(charset.clone())],
        &mut env,
    )
    .unwrap() else {
        panic!("expected string")
    };
    assert_eq!(text.chars().count(), 300);
    assert!(text.chars().all(|c| charset.contains(c)));
    for (name, args) in [
        ("crypto_random_bytes", vec![Value::Int(-1)]),
        ("crypto_random_bytes", vec![Value::Int(i64::MAX)]),
        (
            "crypto_random_string",
            vec![Value::Int(2), Value::Str(String::new())],
        ),
    ] {
        let error = call(name, args, &mut env).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
    let Value::Str(uuid) = call("uuid", vec![], &mut env).unwrap() else {
        panic!("expected UUID")
    };
    assert_eq!(uuid::Uuid::parse_str(&uuid).unwrap().get_version_num(), 4);
}

#[test]
fn agentic_binary_refuses_truncating_codes_and_bytes() {
    let mut env = Env::new();
    for (message, opcode) in [(-1, 0), (256, 0), (0, -1), (0, 256)] {
        let error = call(
            "ab_encode",
            vec![
                Value::Int(message),
                Value::Int(opcode),
                Value::Str("x".into()),
            ],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
    for bytes in [
        vec![],
        vec![Value::Int(256)],
        vec![Value::Int(-1)],
        vec![Value::Int(20), Value::Int(5)],
    ] {
        let error = call("ab_decode", vec![Value::Array(bytes)], &mut env).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn agentic_binary_refuses_overflowing_payload_lengths_without_panicking() {
    for last in [1, 2, 127] {
        let mut bytes = vec![Value::Int(20)];
        bytes.extend(vec![Value::Int(255); 9]);
        bytes.push(Value::Int(last));
        let error = call("ab_decode", vec![Value::Array(bytes)], &mut Env::new()).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn product_overflow_and_non_numeric_cells_are_coded_refusals() {
    for values in [
        vec![Value::Int(i64::MAX), Value::Int(2)],
        vec![Value::Str("x".into())],
    ] {
        let error = call("product", vec![Value::Array(values)], &mut Env::new()).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn sort_key_errors_are_propagated_instead_of_becoming_null() {
    let program = parse_program("[1,2] | sort_by(fn(x) => no_such_key(x))").unwrap();
    let error = eval_program(&program, &mut Env::new()).unwrap_err();
    assert!(error.to_string().contains("no_such_key"), "{error}");
}

#[test]
fn newly_declared_aliases_refuse_discarded_arguments() {
    for name in [
        "from_csv",
        "from-csv",
        "from_yaml",
        "from-yaml",
        "to_yaml",
        "to-yaml",
        "product",
        "concat",
        "reduce",
        "Some",
    ] {
        let args = vec![Value::Null; 8];
        let error = call(name, args, &mut Env::new()).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{name}: {error}");
    }
}

#[test]
fn url_decoding_round_trips_utf8_and_refuses_malformed_data() {
    let mut env = Env::new();
    let text = Value::Str("é 世界".into());
    let encoded = call("web_encode_url", vec![text.clone()], &mut env).unwrap();
    assert_eq!(
        call("web_decode_url", vec![encoded], &mut env).unwrap(),
        text
    );
    for malformed in ["%", "%G0", "%FF"] {
        let error = call(
            "web_decode_url",
            vec![Value::Str(malformed.into())],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn extraction_keeps_a_token_at_the_end_of_input() {
    let mut env = Env::new();
    for (name, text, expected) in [
        ("web_extract_emails", "a@example.com", "a@example.com"),
        ("web_extract_phones", "5551234567", "5551234567"),
    ] {
        assert_eq!(
            call(name, vec![Value::Str(text.into())], &mut env).unwrap(),
            Value::Array(vec![Value::Str(expected.into())])
        );
    }
}

#[test]
fn malformed_json_indexes_do_not_silently_select_zero() {
    let mut env = Env::new();
    for path in ["$[bad]", "$[0", "$[-1]", "$[0]]"] {
        let error = call(
            "web_json_path",
            vec![Value::Array(vec![Value::Int(7)]), Value::Str(path.into())],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn field_previews_do_not_slice_through_a_unicode_character() {
    let mut env = Env::new();
    let text = "界".repeat(60);
    let value = Value::Record(std::collections::BTreeMap::from([(
        "text".into(),
        Value::Str(text),
    )]));
    let Value::Array(fields) = call("type_fields", vec![value], &mut env).unwrap() else {
        panic!("expected fields")
    };
    let Value::Record(field) = &fields[0] else {
        panic!("expected field record")
    };
    let Value::Str(preview) = &field["preview"] else {
        panic!("expected preview")
    };
    assert_eq!(preview.matches('界').count(), 50);
}

#[test]
fn hmac_matches_reference_vectors_and_refuses_invalid_algorithms() {
    let mut env = Env::new();
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("The quick brown fox jumps over the lazy dog".into()),
                Value::Str("key".into()),
                Value::Str("md5".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("80070713463e7749b90c2dc24911e275".into())
    );
    assert_eq!(call("crypto_hmac", vec![Value::Str("Unicode message".into()), Value::Str("KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK".into()), Value::Str("md5".into())], &mut env).unwrap(), Value::Str("1d83714680314ccd9d530f50e8953641".into()));
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("".into()),
                Value::Str("".into()),
                Value::Str("md5".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("74e6f7298a9c2d168935f58c001bad88".into())
    );
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("The quick brown fox jumps over the lazy dog".into()),
                Value::Str("key".into()),
                Value::Str("sha1".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9".into())
    );
    assert_eq!(call("crypto_hmac", vec![Value::Str("Unicode message".into()), Value::Str("KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK".into()), Value::Str("sha1".into())], &mut env).unwrap(), Value::Str("65bd75815d9009090a16197d00b5c95d19982a0e".into()));
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("".into()),
                Value::Str("".into()),
                Value::Str("sha1".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("fbdb1d1b18aa6c08324b7d64b71fb76370690e1d".into())
    );
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("The quick brown fox jumps over the lazy dog".into()),
                Value::Str("key".into()),
                Value::Str("sha256".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8".into())
    );
    assert_eq!(call("crypto_hmac", vec![Value::Str("Unicode message".into()), Value::Str("KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK".into()), Value::Str("sha256".into())], &mut env).unwrap(), Value::Str("ac7de0f744aa272d3f8ba6c8f4be985197401dc443e722b995a2a115cbe8e5e4".into()));
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("".into()),
                Value::Str("".into()),
                Value::Str("sha256".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad".into())
    );
    assert_eq!(call("crypto_hmac", vec![Value::Str("The quick brown fox jumps over the lazy dog".into()), Value::Str("key".into()), Value::Str("sha384".into())], &mut env).unwrap(), Value::Str("d7f4727e2c0b39ae0f1e40cc96f60242d5b7801841cea6fc592c5d3e1ae50700582a96cf35e1e554995fe4e03381c237".into()));
    assert_eq!(call("crypto_hmac", vec![Value::Str("Unicode message".into()), Value::Str("KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK".into()), Value::Str("sha384".into())], &mut env).unwrap(), Value::Str("1ffc837d4db60727c4ebcd6d6beae29a3578acbfb88c661e9b7a9de93526a6287531e51ff43fde78354817e723270718".into()));
    assert_eq!(call("crypto_hmac", vec![Value::Str("".into()), Value::Str("".into()), Value::Str("sha384".into())], &mut env).unwrap(), Value::Str("6c1f2ee938fad2e24bd91298474382ca218c75db3d83e114b3d4367776d14d3551289e75e8209cd4b792302840234adc".into()));
    assert_eq!(call("crypto_hmac", vec![Value::Str("The quick brown fox jumps over the lazy dog".into()), Value::Str("key".into()), Value::Str("sha512".into())], &mut env).unwrap(), Value::Str("b42af09057bac1e2d41708e48a902e09b5ff7f12ab428a4fe86653c73dd248fb82f948a549f7b791a5b41915ee4d1ec3935357e4e2317250d0372afa2ebeeb3a".into()));
    assert_eq!(call("crypto_hmac", vec![Value::Str("Unicode message".into()), Value::Str("KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK".into()), Value::Str("sha512".into())], &mut env).unwrap(), Value::Str("6bf902db15bf2dda9004b0a5e525dd9839cb9ad614d07cda6146d3e29688798c67229e86af510ed96a6dd709fc23c88302ad13d853c1d62454830517db159e11".into()));
    assert_eq!(call("crypto_hmac", vec![Value::Str("".into()), Value::Str("".into()), Value::Str("sha512".into())], &mut env).unwrap(), Value::Str("b936cee86c9f87aa5d3c6f2e84cb5a4239a5fe50480a6ec66b70ab5b1f4ac6730c6c515421b327ec1d69402e53dfb49ad7381eb067b338fd7b0cb22247225d47".into()));
    for algorithm in [Value::Str("unknown".into()), Value::Int(3)] {
        let error = call(
            "crypto_hmac",
            vec![
                Value::Str("message".into()),
                Value::Str("key".into()),
                algorithm,
            ],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn authenticated_crypto_refuses_discarded_options_before_running() {
    let mut env = Env::new();
    for name in ["crypto_encrypt", "crypto_decrypt"] {
        for args in [
            vec![
                Value::Str("message".into()),
                Value::Str("password".into()),
                Value::Str("aes-cbc".into()),
            ],
            vec![Value::Str("message".into()), Value::Int(1)],
            vec![Value::Str("message".into())],
        ] {
            let error = call(name, args, &mut env).unwrap_err();
            assert!(error.to_string().contains("E_BAD_ARG"), "{name}: {error}");
        }
    }
}
