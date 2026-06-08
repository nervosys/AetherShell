//! Evaluating **web stacks** for agentic AI use.
//!
//! Agents do not browse the web a human does; they talk to other services over
//! whatever wire format an LLM-native call graph rewards. That workload has its
//! own five axes — different from the `vms` axes (which score *where* code
//! runs) and different from the language/framework axes (which score *what
//! agents build*). This module scores the **wire protocols and service
//! contracts** an agent actually has to speak with:
//!
//! - **streaming** — does the protocol carry LLM-shaped output (token streams,
//!   latents, mid-stream tool calls) as first-class frames, or is streaming a
//!   bolt-on on top of a document-oriented base?
//! - **tool-discoverability** — can an agent introspect the available
//!   capabilities (tool list, schemas, types) from the protocol itself, or
//!   must it read prose?
//! - **encoding-efficiency** — wire compactness for the LLM/tool-call workload
//!   (binary framing + content-typed payloads vs. JSON-over-HTTP/1.1 baseline).
//! - **interop** — does the agent ecosystem actually speak this? Network
//!   effect: the protocol every SDK already knows is worth more than the
//!   "objectively cleaner" one no one targets.
//! - **security-primitives** — does the protocol carry auth, distributed
//!   tracing, content integrity, and per-message identity natively, or are
//!   they someone-else's-problem?
//!
//! Profiles are curated 0.0–1.0 static judgments with `evidence`, like the
//! [`languages`](crate::languages) / [`frameworks`](crate::frameworks) /
//! [`vms`](crate::vms) profiles — deterministic, serializable, comparable.
//! Scores reflect each stack's design center for *agent-to-service* traffic;
//! a great document-delivery protocol (HTTP+JSON, GraphQL) can rank low for
//! LLM-token streaming and high on interop, and that is the point.
//!
//! ```
//! use agentic_eval::web::{profile, rank_web_stacks, WebStack};
//! let spine = profile(WebStack::Spine);
//! assert!(spine.evidence.len() >= 3);
//! let ranked = rank_web_stacks();
//! assert!(ranked[0].fitness() >= ranked[ranked.len() - 1].fitness());
//! ```

/// Web stacks / wire protocols with curated agentic profiles.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum WebStack {
    /// SPINE (nervosys/SPINE) — the agentic-first web stack. Native
    /// `StreamStart/Token/End` frames with text / bytes / encoded-latent /
    /// tool-call data variants; `CapabilityQuery/CapabilityAdvertisement`
    /// with JSON Schema and optional semantic embeddings; `TraceContext`
    /// (W3C traceparent) attached inline; OpenAI-compatible SSE bridge in
    /// the gateway; bearer auth secure by default as of v1.3.0; optional
    /// FIPS 140-3 build via `aws-lc-rs`.
    Spine,
    /// OpenAI API — HTTP + JSON with SSE chat.completion.chunk streaming;
    /// `tools` parameter for function calling; bearer auth + TLS.
    OpenAiApi,
    /// Anthropic API — HTTP + JSON with SSE `message_start/delta/stop`
    /// streaming; `tools` parameter for tool use; bearer auth + TLS.
    AnthropicApi,
    /// Model Context Protocol — JSON-RPC over stdio or SSE; `tools/list`,
    /// `resources/list`, `prompts/list` introspection RPCs are the surface.
    Mcp,
    /// gRPC — protobuf over HTTP/2 with first-class server / client / bidi
    /// streaming, service reflection, mTLS, interceptors for auth/tracing.
    Grpc,
    /// Plain HTTP + JSON (REST-shaped). The generic baseline an agent has
    /// to fall back to when nothing more specific exists.
    HttpJson,
    /// GraphQL — query language with introspection built into the protocol;
    /// subscriptions for streaming.
    GraphQl,
}

impl WebStack {
    /// All profiled web stacks, in fixed (deterministic) order.
    pub fn all() -> [WebStack; 7] {
        [
            WebStack::Spine,
            WebStack::OpenAiApi,
            WebStack::AnthropicApi,
            WebStack::Mcp,
            WebStack::Grpc,
            WebStack::HttpJson,
            WebStack::GraphQl,
        ]
    }

    /// Canonical lowercase name.
    pub fn name(self) -> &'static str {
        match self {
            WebStack::Spine => "spine",
            WebStack::OpenAiApi => "openai-api",
            WebStack::AnthropicApi => "anthropic-api",
            WebStack::Mcp => "mcp",
            WebStack::Grpc => "grpc",
            WebStack::HttpJson => "http-json",
            WebStack::GraphQl => "graphql",
        }
    }

    /// Parse a (case-insensitive) name; accepts common aliases
    /// (`openai`, `claude`, `model-context-protocol`, `rest`, `graphql-spec`, …).
    pub fn from_name(name: &str) -> Option<WebStack> {
        match name.to_ascii_lowercase().as_str() {
            "spine" | "nervosys-spine" => Some(WebStack::Spine),
            "openai" | "openai-api" | "gpt-api" => Some(WebStack::OpenAiApi),
            "anthropic" | "anthropic-api" | "claude-api" => Some(WebStack::AnthropicApi),
            "mcp" | "model-context-protocol" => Some(WebStack::Mcp),
            "grpc" | "g-rpc" => Some(WebStack::Grpc),
            "http-json" | "rest" | "http+json" | "json-over-http" => Some(WebStack::HttpJson),
            "graphql" | "graphql-spec" | "gql" => Some(WebStack::GraphQl),
            _ => None,
        }
    }
}

/// A curated agentic profile of a web stack / wire protocol across the five
/// agent-native axes, with evidence.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct WebStackProfile {
    /// Which stack this profiles.
    pub stack: WebStack,
    /// LLM-shaped streaming as a first-class frame family
    /// (1.0 = native token / latent / mid-stream tool-call frames).
    pub streaming: f64,
    /// Tool / capability introspection at the protocol layer
    /// (1.0 = the protocol itself exposes a tools/list contract).
    pub tool_discoverability: f64,
    /// Wire compactness for the LLM/tool-call workload
    /// (1.0 = binary framing + content-typed payloads).
    pub encoding_efficiency: f64,
    /// Existing agent-ecosystem adoption
    /// (1.0 = every major SDK already speaks it).
    pub interop: f64,
    /// Auth / tracing / integrity primitives carried by the protocol itself
    /// (1.0 = bearer/mTLS + W3C tracing + content integrity + identity inline).
    pub security_primitives: f64,
    /// Why: one evidence string per notable factor.
    pub evidence: Vec<&'static str>,
}

impl WebStackProfile {
    /// Composite agentic fitness: unweighted mean of all five axes.
    pub fn fitness(&self) -> f64 {
        (self.streaming
            + self.tool_discoverability
            + self.encoding_efficiency
            + self.interop
            + self.security_primitives)
            / 5.0
    }
}

impl std::fmt::Display for WebStackProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: fitness {:.2} (streaming {:.2}, tools {:.2}, encoding {:.2}, interop {:.2}, security {:.2})",
            self.stack.name(),
            self.fitness(),
            self.streaming,
            self.tool_discoverability,
            self.encoding_efficiency,
            self.interop,
            self.security_primitives
        )
    }
}

/// The curated profile for `stack` (static, documented judgments — see module docs).
pub fn profile(stack: WebStack) -> WebStackProfile {
    match stack {
        WebStack::Spine => WebStackProfile {
            stack,
            streaming: 0.97,
            tool_discoverability: 0.95,
            encoding_efficiency: 0.95,
            interop: 0.50,
            security_primitives: 0.95,
            evidence: vec![
                "StreamStart / StreamToken { seq, data, usage? } / StreamEnd are first-class Message variants; StreamData carries Text | Bytes | ToolCall | Encoded(EncodedFrame), so latents and mid-stream function calls fall out of the same frame. v1.5.0 adds Message::StreamCancel (cancel one stream by id — SPINE multiplexes many streams per connection, so closing the socket like SSE is too blunt) and optional StreamToken.usage (mid-stream cumulative token budget, the multiplexed analogue of OpenAI stream_options.include_usage)",
                "two discovery surfaces: native CapabilityQuery (Exact | Prefix | Semantic { embedding, top_k } | All) → CapabilityAdvertisement with input/output JSON Schema per Capability plus optional embedding for similarity-matched lookup; and, as of v1.5.0, the spine_protocol::mcp bridge that re-exposes the same capabilities over the MCP tools/list / tools/call contract — so SPINE matches the introspection gold standard AND adds semantic capability search MCP lacks",
                "as of v1.4.0 the wire body is a self-describing binary codec (8-byte SpineWireHeader + CBOR/RFC 8949, format byte auto-selecting CBOR / CBOR+zstd past 128 B); v1.5.0 serializes EncodedFrame.data and StreamData::Bytes as CBOR byte strings (serde_bytes), bringing tensor payloads to protobuf-class density. Measured (spine-protocol examples/wire_sizes.rs, header included): a 1 KiB embedding frame 3975→446 B (89% smaller), a 2-capability advertisement 806→322 B (60%), a tool call 323→255 B (21%); every frame beats JSON, and EncodedFrame moves raw f32/f16/bf16/q8/q4 tensor bytes zero-token — a path gRPC has no native equivalent for. At parity with protobuf for the agentic data plane",
                "still young (nervosys/SPINE), but no longer isolated: the spine_protocol::mcp bridge is a *runnable* MCP server as of v1.6.0 (mcp::serve_stdio + the mcp_stdio_server example speak the stdio JSON-RPC transport), so a Claude Desktop / Claude Code mcpServers config entry drives a SPINE agent today with zero SPINE-specific code; the OpenAI-compatible /v1/chat/completions + /v1/embeddings + /v1/agentic/{capabilities,codecs} gateway bridges existing SDKs. These are adapters into the two dominant agent contracts rather than native ecosystem adoption — real, deployable reach, but not yet gRPC's install base or OpenAI's universality, which is why interop stays SPINE's lowest axis",
                "message-level security, not just channel: v1.5.0 spine_agentic::signed_frame wraps any frame in an Ed25519 detached signature over the exact wire bytes (integrity + authenticity + non-repudiation, verified before decode) — a guarantee mTLS does not give once a message leaves the socket. Plus W3C TraceContext inline on tool calls/results/stream starts; bearer auth SECURE BY DEFAULT since v1.3.0; zeroize-on-drop on every key-bearing struct; optional FIPS 140-3 build via aws-lc-rs; Chameleon moving-target protocol + Certificate Transparency policy in the box",
            ],
        },
        WebStack::OpenAiApi => WebStackProfile {
            stack,
            streaming: 0.85,
            tool_discoverability: 0.70,
            encoding_efficiency: 0.35,
            interop: 1.00,
            security_primitives: 0.55,
            evidence: vec![
                "SSE chat.completion.chunk is the de facto wire format for LLM tokens; clients consume `data: {...}\\ndata: [DONE]` natively; delta.content + delta.tool_calls handle text and function-call deltas in the same chunk shape",
                "`tools` parameter on the request declares available functions with JSON Schema args — the agent can branch on returned tool_calls — but there is no `tools/list` introspection RPC; discovery is request-time, not protocol-time",
                "JSON over HTTP/1.1 or HTTP/2 — the verbose baseline; no first-class binary or latent path",
                "every major SDK speaks it; LangChain, LlamaIndex, Vercel AI SDK, all OSS agent frameworks default to the OpenAI shape, and most competing providers (Azure, Together, Groq, Fireworks, OpenRouter, …) expose an OpenAI-compatible endpoint as their first interface — the dominant network effect",
                "bearer token + TLS; per-message identity / tracing / integrity are someone-else's problem (use HTTP headers and your own observability stack)",
            ],
        },
        WebStack::AnthropicApi => WebStackProfile {
            stack,
            streaming: 0.85,
            tool_discoverability: 0.70,
            encoding_efficiency: 0.35,
            interop: 0.85,
            security_primitives: 0.55,
            evidence: vec![
                "SSE message_start / content_block_start / content_block_delta / message_delta / message_stop is the streaming protocol; carries text deltas and tool_use blocks; clients consume it the same way they consume OpenAI SSE",
                "`tools` parameter on the request declares tool surface with JSON Schema; tool_use / tool_result blocks complete the loop; no protocol-level tools/list introspection",
                "JSON over HTTPS — same shape as the OpenAI baseline",
                "wide SDK + framework coverage; second-largest closed-LLM ecosystem; some clients reach it through the OpenAI-compatible adapter layer rather than natively",
                "bearer token + TLS; computer use / agent skills add capability surface but auth/tracing/integrity primitives remain transport-level",
            ],
        },
        WebStack::Mcp => WebStackProfile {
            stack,
            streaming: 0.40,
            tool_discoverability: 0.95,
            encoding_efficiency: 0.40,
            interop: 0.65,
            security_primitives: 0.40,
            evidence: vec![
                "JSON-RPC notifications carry tool progress / log entries / sampling — not LLM-token-native; streaming is generic notification flow rather than chat.completion.chunk-shaped",
                "`tools/list`, `resources/list`, `prompts/list`, `tools/call`, `resources/read` are the protocol — discoverability IS the design center; this is the highest-scoring axis for any stack here",
                "JSON-RPC text envelopes over stdio or SSE — verbose like JSON-over-HTTP but with the JSON-RPC frame overhead on top",
                "Anthropic-published in late 2024, adopted by Claude Desktop, Claude Code, several IDE integrations, and growing through 2025-2026; the de facto tool-server contract for agent runtimes",
                "transport-level (stdio process boundary or HTTPS for SSE); no in-protocol auth/tracing/integrity — relies on the host process or HTTP layer",
            ],
        },
        WebStack::Grpc => WebStackProfile {
            stack,
            streaming: 0.70,
            tool_discoverability: 0.85,
            encoding_efficiency: 0.95,
            interop: 0.85,
            security_primitives: 0.80,
            evidence: vec![
                "first-class server / client / bidirectional streaming over HTTP/2 — strong general streaming, but no LLM-token shape out of the box; an agent service has to define its own chunk schema",
                "Server Reflection (proto-reflect) exposes service / method / message descriptors; introspection works but the agent must translate proto types — less direct than tools/list",
                "protobuf binary on HTTP/2 framing — the most compact mainstream wire format; zero JSON envelope overhead",
                "huge enterprise install base; standard for high-throughput internal services and machine-to-machine traffic; well represented in agent backends even if not at the LLM edge",
                "mTLS first-class, per-channel interceptors for auth and OpenTelemetry tracing, deadlines propagate on the wire — among the strongest protocol-level security surfaces in the set",
            ],
        },
        WebStack::HttpJson => WebStackProfile {
            stack,
            streaming: 0.55,
            tool_discoverability: 0.40,
            encoding_efficiency: 0.30,
            interop: 1.00,
            security_primitives: 0.45,
            evidence: vec![
                "chunked Transfer-Encoding + SSE handle streaming as a bolt-on; HTTP/2 server push and HTTP/3 datagrams help, but there is no LLM-token frame standard",
                "OpenAPI/Swagger gives schema discoverability when the service ships one — but the protocol itself does not require it; many real services have no schema endpoint",
                "verbose JSON over HTTP/1.1 (HTTP/2 helps with framing but not body size); the cost baseline every more-efficient stack measures against",
                "the universal lingua franca of services; every language, every framework, every agent stack can call a JSON HTTP endpoint",
                "TLS at transport; auth/tracing/integrity are conventions (bearer headers, W3C traceparent, content hashing) layered on, not in the protocol",
            ],
        },
        WebStack::GraphQl => WebStackProfile {
            stack,
            streaming: 0.50,
            tool_discoverability: 0.95,
            encoding_efficiency: 0.35,
            interop: 0.75,
            security_primitives: 0.45,
            evidence: vec![
                "Subscriptions handle streaming as a separate operation type over WebSocket / SSE; not LLM-token native; defer / stream directives help for partial results",
                "introspection (__schema, __type) is built into the protocol — a client can discover the entire surface without docs; on this axis GraphQL ties MCP for the highest score",
                "JSON request/response with selection sets that reduce over-fetch; binary efficiency is still text-JSON-shaped",
                "wide adoption especially on the front-end and federated-service edges; persisted-query patterns are common in agent backends",
                "TLS at transport; persisted queries help control surface area; per-message identity / tracing / integrity remain conventions",
            ],
        },
    }
}

/// Profiles for all stacks, in [`WebStack::all`] order (deterministic).
pub fn profiles() -> Vec<WebStackProfile> {
    WebStack::all().iter().map(|&s| profile(s)).collect()
}

/// All profiles ranked best-first by [`WebStackProfile::fitness`]
/// (stable order on ties).
pub fn rank_web_stacks() -> Vec<WebStackProfile> {
    let mut v = profiles();
    v.sort_by(|a, b| {
        b.fitness()
            .partial_cmp(&a.fitness())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    v
}

/// Compare two stacks: positive deltas mean `a` fits agentic use better.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct WebStackComparison {
    /// First stack (the subject).
    pub a: WebStackProfile,
    /// Second stack (the baseline).
    pub b: WebStackProfile,
    /// `a.fitness() - b.fitness()`.
    pub fitness_delta: f64,
    /// Axis name → delta (a − b), in fixed axis order.
    pub axis_deltas: Vec<(&'static str, f64)>,
}

/// Compare stack `a` against baseline `b` across all five axes.
pub fn compare_web_stacks(a: WebStack, b: WebStack) -> WebStackComparison {
    let pa = profile(a);
    let pb = profile(b);
    let axis_deltas = vec![
        ("streaming", pa.streaming - pb.streaming),
        (
            "tool-discoverability",
            pa.tool_discoverability - pb.tool_discoverability,
        ),
        (
            "encoding-efficiency",
            pa.encoding_efficiency - pb.encoding_efficiency,
        ),
        ("interop", pa.interop - pb.interop),
        (
            "security-primitives",
            pa.security_primitives - pb.security_primitives,
        ),
    ];
    WebStackComparison {
        fitness_delta: pa.fitness() - pb.fitness(),
        a: pa,
        b: pb,
        axis_deltas,
    }
}

impl std::fmt::Display for WebStackComparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} vs {}: fitness delta {:+.2}",
            self.a.stack.name(),
            self.b.stack.name(),
            self.fitness_delta
        )?;
        for (axis, d) in &self.axis_deltas {
            writeln!(f, "  {axis}: {d:+.2}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stack_profiles_with_evidence() {
        for stack in WebStack::all() {
            let p = profile(stack);
            assert!(
                p.evidence.len() >= 3,
                "{} needs ≥3 evidence lines",
                stack.name()
            );
            for s in [
                p.streaming,
                p.tool_discoverability,
                p.encoding_efficiency,
                p.interop,
                p.security_primitives,
            ] {
                assert!(
                    (0.0..=1.0).contains(&s),
                    "{} score out of range",
                    stack.name()
                );
            }
        }
    }

    #[test]
    fn from_name_roundtrip_and_aliases() {
        for stack in WebStack::all() {
            assert_eq!(WebStack::from_name(stack.name()), Some(stack));
        }
        assert_eq!(WebStack::from_name("OpenAI"), Some(WebStack::OpenAiApi));
        assert_eq!(WebStack::from_name("claude-api"), Some(WebStack::AnthropicApi));
        assert_eq!(WebStack::from_name("REST"), Some(WebStack::HttpJson));
        assert_eq!(
            WebStack::from_name("model-context-protocol"),
            Some(WebStack::Mcp)
        );
        assert_eq!(WebStack::from_name("gql"), Some(WebStack::GraphQl));
        assert_eq!(WebStack::from_name("nervosys-spine"), Some(WebStack::Spine));
        assert_eq!(WebStack::from_name("not-a-stack"), None);
    }

    #[test]
    fn ranking_is_deterministic_and_sorted() {
        let r1 = rank_web_stacks();
        let r2 = rank_web_stacks();
        let n1: Vec<_> = r1.iter().map(|p| p.stack.name()).collect();
        let n2: Vec<_> = r2.iter().map(|p| p.stack.name()).collect();
        assert_eq!(n1, n2);
        for w in r1.windows(2) {
            assert!(w[0].fitness() >= w[1].fitness());
        }
    }

    #[test]
    fn axis_judgments_hold_directionally() {
        let spine = profile(WebStack::Spine);
        let openai = profile(WebStack::OpenAiApi);
        let anthropic = profile(WebStack::AnthropicApi);
        let mcp = profile(WebStack::Mcp);
        let grpc = profile(WebStack::Grpc);
        let http = profile(WebStack::HttpJson);
        let graphql = profile(WebStack::GraphQl);

        // SPINE leads on the axes it was designed for.
        assert!(
            spine.streaming > anthropic.streaming,
            "native StreamStart/Token/End frames beat SSE-on-HTTP for LLM streaming"
        );
        assert!(
            spine.security_primitives > openai.security_primitives,
            "inline W3C tracing + zeroize + secure-by-default auth + Chameleon protocol beat bearer-on-TLS"
        );

        // SPINE pays for being new.
        assert!(
            openai.interop > spine.interop,
            "the OpenAI API has the dominant ecosystem network effect; SPINE is brand new"
        );
        assert!(
            http.interop >= openai.interop,
            "plain HTTP+JSON is the universal lingua franca"
        );

        // gRPC's general strengths.
        assert!(
            grpc.encoding_efficiency > openai.encoding_efficiency,
            "protobuf binary beats verbose JSON over HTTP"
        );

        // SPINE's CBOR wire format (v1.4.0) plus byte-string tensor payloads
        // (v1.5.0) moved encoding from a weakness to protobuf-class density.
        assert!(
            spine.encoding_efficiency > openai.encoding_efficiency,
            "binary CBOR (+opportunistic zstd) crushes JSON-over-HTTP"
        );
        assert!(
            spine.encoding_efficiency > mcp.encoding_efficiency,
            "binary CBOR beats JSON-RPC text envelopes"
        );
        assert!(
            spine.encoding_efficiency >= grpc.encoding_efficiency,
            "byte-string tensor payloads bring SPINE to parity with protobuf for the agentic data plane"
        );
        assert!(
            grpc.security_primitives > http.security_primitives,
            "mTLS + interceptors beat bring-your-own bearer-headers"
        );

        // v1.5.0 lifted every axis with real capabilities (MCP bridge for
        // interop + tools, byte-string payloads for encoding, per-message
        // Ed25519 signatures for security, StreamCancel/usage for streaming),
        // so SPINE now edges gRPC on the composite — while STILL trailing badly
        // on interop, the one axis that only rewards real ecosystem adoption.
        assert!(
            spine.fitness() > grpc.fitness(),
            "v1.5.0 capability work puts SPINE first on composite agentic fitness"
        );
        assert!(
            grpc.interop > spine.interop,
            "SPINE reaches the ecosystem through MCP/OpenAI bridges, not native adoption; gRPC's install base is broader"
        );
        assert!(
            spine.security_primitives > grpc.security_primitives,
            "per-message Ed25519 signatures (non-repudiation) exceed channel-only mTLS"
        );

        // MCP / GraphQL win on introspection because the protocol IS the schema.
        assert!(
            mcp.tool_discoverability > openai.tool_discoverability,
            "tools/list is more discoverable than a request-time tools parameter"
        );
        assert!(
            graphql.tool_discoverability > http.tool_discoverability,
            "__schema introspection beats hoping the service ships OpenAPI"
        );

        // MCP is tool-shaped, not LLM-token-shaped.
        assert!(
            openai.streaming > mcp.streaming,
            "JSON-RPC notifications are not LLM-token streaming"
        );
    }

    #[test]
    fn comparison_deltas_are_consistent() {
        let cmp = compare_web_stacks(WebStack::Spine, WebStack::OpenAiApi);
        let sum: f64 = cmp.axis_deltas.iter().map(|(_, d)| d).sum();
        assert!((sum / 5.0 - cmp.fitness_delta).abs() < 1e-9);
        assert!(format!("{cmp}").contains("spine vs openai-api"));
    }
}
