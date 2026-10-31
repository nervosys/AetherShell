# AI Integration

IronShell has a built-in `ai()` function. It sends one prompt to the provider
named by `IRONSHELL_AI`, and returns the reply as a string. For tool use, see
`agent` in [Creating Agents](./agents.md).

## Quick Start

```ironshell
# Simple AI query
ai("What is the capital of France?")
# → "The capital of France is Paris."
```

## Signature

```
ai(prompt, config?)  ->  String
```

- `prompt` is a String. With piped input, the piped string is the prompt and
  the first argument is ignored.
- `config` is an optional record. It accepts `images`, `audio` and `video`, each
  an array of readable file paths that are sent with the prompt. A `model`
  field is accepted and not used.
- At most 30 calls per minute per process.

## Choosing a Provider

`ai()` takes no model argument. The provider is read from the `IRONSHELL_AI`
environment variable, and the call fails with "no AI provider is configured"
when it is unset.

```bash
export IRONSHELL_AI=openai
export OPENAI_API_KEY=sk-...
```

Names the router accepts include `openai`, `anthropic`, `ollama`, `vllm`,
`llamacpp`, `tgi`, `lmstudio`, `compat`, `openrouter`, `groq`, `mistral`,
`google`, `azure`, and `irongate`. The full list is in `complete_sync_router`
in `src/ai.rs`.

## Context

`ai()` does not read a `context` field. To give the model a file, read it into
a variable and put it in the prompt:

```ironshell
code = file.read("src/main.rs")
ai("Review this code for security issues: ${code}")
```

## Multimodal

```ironshell
ai("What's in this image?", {images: ["photo.jpg"]})
ai("Transcribe this", {audio: ["meeting.mp3"]})
ai("Summarize this clip", {video: ["demo.mp4"]})
```

## Not Supported

Temperature, `max_tokens`, system prompts, message history, JSON schema output,
streaming and tool definitions are not parameters of `ai()`. A call that passes
them gets no effect from them.

## Error Handling

```ironshell
let answer = try { ai("Query that might fail") } catch err { "fallback response" }
```

## Best Practices

1. **Set the provider** with `IRONSHELL_AI` and its API key before starting `ish`.
2. **Keep prompts deliberate** - put only the file text you need into the prompt.
3. **Handle errors** - provider calls can fail; wrap them in `try`.
