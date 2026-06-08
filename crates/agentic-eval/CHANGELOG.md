# Changelog — agentic-eval

All notable changes to the `agentic-eval` crate. Follows
[Keep a Changelog](https://keepachangelog.com/) and [SemVer](https://semver.org/).

## [0.10.0] - 2026-06-08

### Changed
- **SPINE re-scored on `encoding-efficiency`: 0.65 → 0.92**, from measured
  data after SPINE v1.4.0 shipped a binary wire format. The body is no longer
  serde_json behind a header — it is a self-describing CBOR codec (8-byte
  `SpineWireHeader` + CBOR/RFC 8949, with opportunistic zstd past 128 B).
  Measured against the old JSON body (`spine-protocol`
  `examples/wire_sizes.rs`, header included): a 1 KiB embedding frame
  3975→546 B (86% smaller), a 2-capability advertisement 806→322 B (60%), a
  tool call 323→255 B (21%); every frame beats JSON. SPINE now sits in the
  binary-efficient tier next to gRPC/protobuf (0.95) — gRPC stays marginally
  ahead because protobuf elides field names while CBOR keeps self-describing
  keys, but SPINE's `EncodedFrame` moves raw tensor bytes zero-token, which
  protobuf has no native equivalent for. Evidence string and the
  `axis_judgments_hold_directionally` directional test updated accordingly;
  the honest `interop` 0.15 is unchanged (interop is a later concern).

## [0.9.0] - 2026-06-04

### Added
- **`web`** — curated agentic profiles of 7 **web stacks / wire protocols**
  (SPINE, OpenAI API, Anthropic API, MCP, gRPC, HTTP+JSON, GraphQL) for the
  *agent-to-service* traffic an agent actually has to speak. Scored on five
  agent-native axes distinct from the VM axes (since a wire protocol is not
  a sandbox): **streaming** (LLM-shaped output as a first-class frame family
  vs. a bolt-on on top of a document protocol), **tool-discoverability**
  (introspect the surface from the protocol itself vs. read prose),
  **encoding-efficiency** (binary framing + content-typed payloads vs.
  JSON-over-HTTP/1.1 baseline), **interop** (does the agent ecosystem already
  speak it?), and **security-primitives** (auth, W3C distributed tracing,
  content integrity inline vs. someone-else's-problem). Each profile carries
  ≥3 evidence strings; `profile` / `profiles` / `rank_web_stacks` /
  `compare_web_stacks`, `WebStack::from_name` aliases (`openai`, `claude-api`,
  `mcp` / `model-context-protocol`, `rest`, `gql`, `nervosys-spine`). Wired
  into the self-describing ontology: `describe("web")`, `describe("spine")`,
  `describe("grpc")`, and the `manifest()` index now lists web stacks.
- `examples/web_benchmark.rs` — ranked table + SPINE-vs-OpenAI head-to-head +
  SPINE evidence dump + reading summary. Run with
  `cargo run -p agentic-eval --example web_benchmark`.

### Notes
- **SPINE is evaluated honestly**: it leads on the protocol-semantics axes it
  was designed for (streaming, tool-discoverability, security-primitives) and
  carries an explicit 0.15 on `interop` for being brand new — the gateway's
  OpenAI-compatible `/v1/chat/completions`, `/v1/embeddings`, and
  `/v1/agentic/{capabilities,codecs}` routes are documented as the migration
  bridge. gRPC tops the unweighted composite because its broad strengths
  (protobuf efficiency + mTLS + reflection + bidi streaming + huge install
  base) outweigh SPINE's narrower edge on LLM-native frames; that ranking is
  the point of the benchmark.

## [0.8.0] - 2026-06-03

### Added
- **`vms`** — curated agentic profiles of 7 VM/sandbox systems (AetherVM,
  Firecracker, Cloud Hypervisor, gVisor, Kata Containers, QEMU/KVM, Docker) for
  the *ephemeral agent-sandbox* workload an agent runtime drives. Scored on five
  **agent-native axes** (distinct from the program axes, since a VM isn't text):
  **start-latency** (cold-start per tool call), **density** (sandboxes per host),
  **isolation** (boundary strength for untrusted agent-generated code),
  **snapshotting** (CoW fork / warm-pool branching), and **agent-control**
  (tool/MCP-native control plane vs. bring-your-own glue). Each profile carries
  evidence strings; `profile`/`profiles`/`rank_vms`/`compare_vms`,
  `Vm::from_name` aliases (`fc`, `chv`, `runsc`, `kvm`, `runc`, `hypermachine`).
  Wired into the self-describing ontology: `describe("vms")`,
  `describe("firecracker")`, and the `manifest()` index now lists VM systems.

### Notes
- The VM axes are workload-specific by design: a strong long-lived datacenter VM
  can rank low for the spawn-and-tear-down sandbox loop, and a shared-kernel
  container ranks high on speed/density but low on isolation for untrusted code.
  Scores are honest curated judgments with rationale — including AetherVM's
  (strong on snapshotting/agent-control, with an explicit "younger, less
  battle-tested at scale" caveat on isolation).

## [0.7.0] - 2026-06-03

### Added
Two new evaluation **subjects** — beyond programs, the crate now profiles what
agents *build with*:
- **`languages`** — curated agentic profiles of 10 programming languages
  (Python, Rust, JS, TS, Go, Bash, C, C++, Java, MechGen) on the four axes:
  token efficiency, determinism, reliability (does the toolchain catch agent
  mistakes with actionable diagnostics?), and safety (default blast radius).
  Each profile carries evidence strings; `profile`/`profiles`/`rank_languages`/
  `compare_languages`, `Language::from_name` aliases (`js`, `c++`, `golang`, …).
- **`frameworks`** — curated agentic profiles of 9 AI frameworks (PyTorch,
  TensorFlow, JAX, HF Transformers, ONNX Runtime, scikit-learn, Candle, Burn,
  RecursiveMachineIntelligence (RMI)) on the four axes **plus discoverability** (can an agent learn
  the surface from the framework itself — schemas/ontology/introspection — vs
  scraping prose?). Notes artifact-safety facts (pickle ≈ arbitrary code,
  `trust_remote_code`, safetensors). `profile`/`rank_frameworks`/
  `compare_frameworks`, `Framework::from_name` aliases (`torch`, `tf`, `hf`, `rmi`).

Both are static curated judgments (deterministic, serializable, with rationale),
not measurements — use the program-level axes to measure your own code. Wired
into the self-describing ontology: `manifest()` lists both groups;
`describe("languages")`/`describe("rust")`/`describe("pytorch")` expand them
(ranked tables / full profiles + evidence). All types re-exported at the root.

## [0.6.0] - 2026-06-03

### Added
Five new metrics across the cost/reliability/safety axes (each: typed report,
`Display`, `serde`, ontology entry, tests):
- **Token cost — output scaling** (`assess_scaling`, `ScalingReport`): least-squares
  fit of output tokens vs result size → marginal `per_item` cost + `fixed_overhead`;
  flags O(1) output. The curve that matters at agent scale, not a single sample.
- **Token cost — prompt-cache efficiency** (`assess_cache`, `CacheReport`,
  `cacheable_prefix_tokens`): models API prompt-caching — a stable prefix paid once
  at write price (×1.25) then read price (×0.1) — reporting `cacheable_ratio` and the
  session savings ratio.
- **Reliability — graded error actionability** (`assess_error_quality`,
  `ErrorQuality`/`ErrorQualityReport`): refines the binary actionable flag into a
  0–1 score over code/message/location/fix.
- **Safety — reversibility** (`assess_reversibility`, `ReversibilityReport`): fraction
  of *dangerous* effects backed by undo/rollback — the recoverable-blast-radius
  complement to gating.
- **Safety — exfiltration risk** (`assess_exfiltration`, `ExfiltrationReport`):
  source∧sink exposure — reads local state *and* has a network/exec egress path.

All re-exported at the crate root and listed in the self-describing `ontology`.

## [0.5.0] - 2026-06-02

### Added
- **`ontology` module — a complete, self-describing ontology over the crate.**
  Agentic-first: a consumer discovers the whole surface from a compact, deterministic
  `manifest()` (axes, effect taxonomy, modes, models, command count — a few hundred
  tokens) and expands any entry with `describe("<axis|effect|model|section>")`, the
  same progressive-disclosure pattern the crate measures. `ontology()` returns the
  full structured catalog (`Ontology`, `serde`-serializable): the four axes with
  entry points/output types, every `Effect` with its per-`Mode` policy `Decision` and
  example commands, the tokenizer `Model`s with exactness, and the classifier size.
- **Taxonomy enumerators** for building ontologies over the types: `Effect::all`,
  `Effect::summary`, `Effect::decision`, `Mode::all`, `Mode::name`, `Decision::name`,
  and `commands::commands_for` / `commands::known_command_count`.

### Changed
- Crate-level docs now state the agentic-first design contract (deterministic,
  execution-agnostic, structured, self-describing).

## [0.4.0] - 2026-06-02

### Added
- **`commands` module — heuristic CLI effect classification.** A curated table of
  ~200 common POSIX/Unix/dev tools mapped to their [`safety::Effect`] class, so the
  safety axis works on a **wide variety of real CLI programs** without a hand-written
  classifier: `classify_command` (name → effect), `classify_invocation` (one command
  line — strips `VAR=val` and path prefixes; unknown program → `Exec`; `sudo`/`doas`
  → `Privileged`), `classify_script` (split a script on `\n ; | & && ||` and classify
  each), and `assess_safety_script` (one call from a script to a `SafetyReport`).
  Fail-safe by design: an unrecognized program is treated as arbitrary execution, and
  multi-mode tools map to their most security-salient common effect (`git` → network,
  `docker`/`npm`/`make` → exec, `apt`/`mount` → privileged). All re-exported at the
  crate root.

## [0.3.0] - 2026-06-02

### Added
- **Pluggable tokenizer**: `tokens::evaluate_with` and `rank_with` accept any
  `Fn(&str) -> usize`, so a host can flow its own exact tokenizer through the cost
  model instead of the built-in heuristic/BPEs.
- `AgentCost::total_standing_per_turn` — the no-prompt-caching cost model (standing
  context re-sent every turn), complementing the caching-amortized `total_over`.
- `safety::assess_safety_named` — score safety from operation names plus a classifier
  closure (unknowns skipped).
- Release hardening: `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]` (every public
  item is documented), full `Cargo.toml` metadata (`readme`, `documentation`,
  `rust-version`, docs.rs all-features), and a crate CHANGELOG.

## [0.2.0] - 2026-06-02

### Added
- Crate-root re-exports of the most-used types (`Model`, `Program`, `AgentCost`,
  `Effect`, `Mode`, `assess_*`, …).
- `Display` for every report type (`AgentCost`, `Comparison`, `DeterminismReport`,
  `ReliabilityReport`, `SafetyReport`, `Evaluation`).
- Optional `serde` feature deriving `Serialize` on all report/config types.
- `Model::from_name` (CLI/config parsing), `tokens::rank` (N-way comparison),
  `Evaluation` `with_*` builders, `safety::Effect::from_name`.

### Changed
- The heuristic tokenizer splits `snake_case` subwords (`file_read` ≈ 2 tokens).
- Corrected the `AnthropicClaude` model docs to state it is a heuristic approximation
  (no public offline Claude tokenizer), not a "calibrated" estimate.

## [0.1.0] - 2026-06-01

### Added
- Initial release. Four-axis evaluation of programs for agentic AI use:
  **token efficiency** (`tokens`), **determinism** (`determinism`), **reliability**
  (`reliability`), and **safety** (`safety`), plus a combined `Evaluation`.
- Token counting under OpenAI GPT-4 (`cl100k`) and GPT-4o (`o200k`) — exact with
  `--features real-tokens`, a documented Anthropic-Claude approximation, and a
  labeled heuristic otherwise. Execution-agnostic; zero heavy deps by default.
