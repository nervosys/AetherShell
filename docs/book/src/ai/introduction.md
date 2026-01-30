# AI Integration

AetherShell has first-class support for AI models and agents.

## Quick Start

```aethershell
# Simple AI query
ai("What is the capital of France?")
# → "The capital of France is Paris."

# With specific model
ai("Explain monads in simple terms", {
    model: "gpt-4o"
})
```

## Supported Providers

AetherShell supports 25+ AI providers out of the box:

| Provider       | Models                     | Setup                |
| -------------- | -------------------------- | -------------------- |
| **OpenAI**     | GPT-4o, GPT-4, GPT-3.5     | `OPENAI_API_KEY`     |
| **Anthropic**  | Claude 3 Opus/Sonnet/Haiku | `ANTHROPIC_API_KEY`  |
| **Google**     | Gemini Pro, Gemini Flash   | `GOOGLE_API_KEY`     |
| **Meta**       | Llama 3, CodeLlama         | Via Ollama/Together  |
| **Mistral**    | Mistral Large, Codestral   | `MISTRAL_API_KEY`    |
| **Cohere**     | Command R, Command R+      | `COHERE_API_KEY`     |
| **xAI**        | Grok                       | `XAI_API_KEY`        |
| **DeepSeek**   | DeepSeek V3, R1            | `DEEPSEEK_API_KEY`   |
| **Ollama**     | Any local model            | Local install        |
| **OpenRouter** | 100+ models                | `OPENROUTER_API_KEY` |

## Model URIs

Specify models using the `provider:model` format:

```aethershell
# OpenAI
ai("Query", { model: "openai:gpt-4o-mini" })

# Anthropic
ai("Query", { model: "claude:claude-3-sonnet-20240229" })

# Google
ai("Query", { model: "gemini:gemini-pro" })

# Local Ollama
ai("Query", { model: "ollama:llama3" })

# OpenRouter (any model)
ai("Query", { model: "openrouter:meta-llama/llama-3-70b-instruct" })
```

## AI Function Options

```aethershell
ai("Your prompt", {
    # Model selection
    model: "gpt-4o",
    
    # Generation parameters
    temperature: 0.7,      # Creativity (0-2)
    max_tokens: 4096,      # Response length limit
    top_p: 0.95,           # Nucleus sampling
    
    # Context
    system: "You are a helpful assistant",  # System prompt
    context: read("data.txt"),              # Additional context
    
    # Output format
    format: "json",        # Request JSON output
    stream: true,          # Stream response
    
    # Images (multimodal)
    images: ["image.png"],
})
```

## Conversation History

Maintain context across queries:

```aethershell
let history = []

let chat = fn(message) => {
    let response = ai(message, {
        messages: history,
        model: "gpt-4o"
    })
    
    # Update history
    history = [...history, 
        { role: "user", content: message },
        { role: "assistant", content: response }
    ]
    
    response
}

chat("What is Rust?")
chat("How does it handle memory?")  # Remembers context
```

## Multimodal (Vision)

Analyze images with vision-capable models:

```aethershell
# Describe an image
ai("What's in this image?", {
    model: "gpt-4o",
    images: ["photo.jpg"]
})

# Multiple images
ai("Compare these two images", {
    model: "claude:claude-3-sonnet",
    images: ["before.png", "after.png"]
})

# URL images
ai("Analyze this diagram", {
    model: "gemini:gemini-pro-vision",
    images: ["https://example.com/diagram.png"]
})
```

## Structured Output

Get structured JSON responses:

```aethershell
let result = ai("Extract the person's name and age from: John is 30 years old", {
    model: "gpt-4o",
    format: "json",
    schema: {
        type: "object",
        properties: {
            name: { type: "string" },
            age: { type: "integer" }
        }
    }
})

let data = json_parse(result)
print(data.name)  # "John"
print(data.age)   # 30
```

## Error Handling

```aethershell
let result = try {
    ai("Query that might fail", { model: "gpt-4o" })
} catch err {
    print("AI error: " + err.message)
    "fallback response"
}
```

## Provider-Specific Features

### OpenAI Function Calling

```aethershell
let tools = [
    {
        name: "get_weather",
        description: "Get current weather for a location",
        parameters: {
            type: "object",
            properties: {
                location: { type: "string" }
            }
        }
    }
]

let response = ai("What's the weather in Paris?", {
    model: "gpt-4o",
    tools: tools
})
```

### Claude with System Prompts

```aethershell
ai("Translate to French: Hello, world!", {
    model: "claude:claude-3-haiku",
    system: "You are a professional translator. Respond only with the translation."
})
```

### Local Models with Ollama

```bash
# First, pull the model
ollama pull llama3
ollama pull codellama
```

```aethershell
# Use in AetherShell
ai("Write a Python function to sort a list", {
    model: "ollama:codellama"
})
```

## Best Practices

1. **Set defaults** - Configure your preferred provider:
   ```bash
   export AETHER_AI=openai
   export OPENAI_API_KEY=sk-...
   ```

2. **Use appropriate models** - GPT-4o for complex tasks, GPT-3.5 for simple ones

3. **Control costs** - Set `max_tokens` to limit response length

4. **Handle errors** - AI APIs can fail; always have fallbacks

5. **Stream long responses** - Set `stream: true` for better UX
