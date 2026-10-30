/**
 * IronShell Agent API - TypeScript Integration Example
 * 
 * This example demonstrates how to integrate IronShell's Agent API with
 * popular TypeScript AI frameworks like OpenAI, Anthropic, and Vercel AI SDK.
 * 
 * Prerequisites:
 *   npm install openai @anthropic-ai/sdk ai
 * 
 * Start the Agent API server:
 *   ish --agent-api
 */

// =============================================================================
// IronShell Agent API Client
// =============================================================================

interface AgentResponse<T = unknown> {
    success: boolean;
    result?: T;
    result_type?: string;
    error?: string;
}

interface BuiltinInfo {
    name: string;
    description: string;
    category?: string;
}

interface SchemaResult {
    format: string;
    compatible_models: string[];
    tools?: OpenAITool[];
    function_declarations?: GeminiFunctionDeclaration[];
    [key: string]: unknown;
}

interface OpenAITool {
    type: "function";
    function: {
        name: string;
        description: string;
        parameters: Record<string, unknown>;
    };
}

interface GeminiFunctionDeclaration {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
}

class IronShellClient {
    private baseUrl: string;

    constructor(baseUrl: string = "http://localhost:3002") {
        this.baseUrl = baseUrl;
    }

    private async post<T>(data: Record<string, unknown>): Promise<AgentResponse<T>> {
        const response = await fetch(this.baseUrl, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(data),
        });
        return response.json();
    }

    async eval(code: string): Promise<AgentResponse> {
        return this.post({ action: "eval", code });
    }

    async call(builtin: string, args?: Record<string, unknown>): Promise<AgentResponse> {
        const data: Record<string, unknown> = { action: "call", builtin };
        if (args) data.args = args;
        return this.post(data);
    }

    async listBuiltins(category?: string): Promise<AgentResponse<{ builtins: BuiltinInfo[] }>> {
        const data: Record<string, unknown> = { action: "list_builtins" };
        if (category) data.category = category;
        return this.post(data);
    }

    async describe(builtin: string): Promise<AgentResponse> {
        return this.post({ action: "describe", builtin });
    }

    async schema(format: string): Promise<AgentResponse<SchemaResult>> {
        return this.post({ action: "schema", format });
    }

    async pipeline(steps: Array<Record<string, unknown>>): Promise<AgentResponse> {
        return this.post({ action: "pipeline", steps });
    }
}

// =============================================================================
// OpenAI Integration
// =============================================================================

async function openaiIntegrationExample(): Promise<void> {
    const OpenAI = await import("openai").then((m) => m.default);

    // Initialize clients
    const ironshell = new IronShellClient();
    const openai = new OpenAI(); // Uses OPENAI_API_KEY env var

    // Get IronShell tool schema in OpenAI format
    const schemaResponse = await ironshell.schema("openai");
    if (!schemaResponse.success || !schemaResponse.result) {
        console.error("Failed to get schema:", schemaResponse.error);
        return;
    }

    const tools = schemaResponse.result.tools || [];
    console.log(`Loaded ${tools.length} tools for OpenAI`);
    console.log(`Compatible models: ${schemaResponse.result.compatible_models.join(", ")}`);

    // Create chat completion with tools
    const response = await openai.chat.completions.create({
        model: "gpt-4o-mini", // or "gpt-5", "o3", "o4-mini"
        messages: [
            { role: "system", content: "You are a helpful shell assistant. Use tools to help users." },
            { role: "user", content: "List files in the current directory" },
        ],
        tools: tools as any,
        tool_choice: "auto",
    });

    // Handle tool calls
    const message = response.choices[0].message;
    if (message.tool_calls) {
        for (const toolCall of message.tool_calls) {
            const functionName = toolCall.function.name;
            const functionArgs = JSON.parse(toolCall.function.arguments);

            console.log(`AI requested: ${functionName}(${JSON.stringify(functionArgs)})`);

            // Execute via IronShell
            const result = await ironshell.call(functionName, functionArgs);
            if (result.success) {
                console.log("Result:", result.result);
            } else {
                console.error("Error:", result.error);
            }
        }
    } else {
        console.log("AI response:", message.content);
    }
}

// =============================================================================
// Anthropic/Claude Integration
// =============================================================================

async function anthropicIntegrationExample(): Promise<void> {
    const Anthropic = await import("@anthropic-ai/sdk").then((m) => m.default);

    // Initialize clients
    const ironshell = new IronShellClient();
    const anthropic = new Anthropic(); // Uses ANTHROPIC_API_KEY env var

    // Get IronShell tool schema in Claude format
    const schemaResponse = await ironshell.schema("claude");
    if (!schemaResponse.success || !schemaResponse.result) {
        console.error("Failed to get schema:", schemaResponse.error);
        return;
    }

    const tools = schemaResponse.result.tools || [];
    console.log(`Loaded ${tools.length} tools for Claude`);
    console.log(`Compatible models: ${schemaResponse.result.compatible_models.join(", ")}`);

    // Create message with tools
    const message = await anthropic.messages.create({
        model: "claude-3-5-sonnet-20241022", // or claude-4.5-opus, claude-4.5-sonnet
        max_tokens: 1024,
        tools: tools as any,
        messages: [{ role: "user", content: "What files are in the current directory?" }],
    });

    // Handle tool use
    for (const block of message.content) {
        if (block.type === "tool_use") {
            const toolName = block.name;
            const toolInput = block.input as Record<string, unknown>;

            console.log(`Claude requested: ${toolName}(${JSON.stringify(toolInput)})`);

            // Execute via IronShell
            const result = await ironshell.call(toolName, toolInput);
            if (result.success) {
                console.log("Result:", result.result);
            } else {
                console.error("Error:", result.error);
            }
        } else if (block.type === "text") {
            console.log("Claude:", block.text);
        }
    }
}

// =============================================================================
// Vercel AI SDK Integration
// =============================================================================

async function vercelAiIntegrationExample(): Promise<void> {
    const { generateText, tool } = await import("ai");
    const { openai } = await import("@ai-sdk/openai");

    // Initialize IronShell client
    const ironshell = new IronShellClient();

    // Get builtin descriptions
    const builtinsResponse = await ironshell.listBuiltins();
    if (!builtinsResponse.success || !builtinsResponse.result) {
        console.error("Failed to list builtins");
        return;
    }

    // Create Vercel AI SDK tools from IronShell builtins
    const tools: Record<string, any> = {};

    for (const builtin of builtinsResponse.result.builtins.slice(0, 5)) {
        tools[builtin.name] = tool({
            description: builtin.description,
            parameters: {}, // Simplified for demo
            execute: async (args: Record<string, unknown>) => {
                const result = await ironshell.call(builtin.name, args);
                if (result.success) {
                    return JSON.stringify(result.result);
                }
                throw new Error(result.error || "Unknown error");
            },
        });
    }

    // Generate text with tools
    const result = await generateText({
        model: openai("gpt-4o-mini"),
        tools,
        prompt: "List files in the current directory",
    });

    console.log("Generated text:", result.text);
    console.log("Tool calls:", result.toolCalls);
    console.log("Tool results:", result.toolResults);
}

// =============================================================================
// Direct API Usage Examples
// =============================================================================

async function directApiExamples(): Promise<void> {
    const ironshell = new IronShellClient();

    console.log("=== Direct API Examples ===\n");

    // 1. Evaluate code
    console.log("1. Evaluate expression:");
    const evalResult = await ironshell.eval("1 + 2 * 3");
    console.log(`   1 + 2 * 3 = ${evalResult.result}`);

    // 2. List builtins by category
    console.log("\n2. List filesystem builtins:");
    const builtinsResult = await ironshell.listBuiltins("filesystem");
    if (builtinsResult.success && builtinsResult.result) {
        for (const b of builtinsResult.result.builtins.slice(0, 5)) {
            console.log(`   - ${b.name}: ${b.description}`);
        }
    }

    // 3. Call builtin
    console.log("\n3. Call 'pwd' builtin:");
    const pwdResult = await ironshell.call("pwd");
    if (pwdResult.success) {
        console.log(`   Current directory: ${pwdResult.result}`);
    }

    // 4. Execute pipeline
    console.log("\n4. Execute pipeline (ls | take | select):");
    const pipelineResult = await ironshell.pipeline([
        { builtin: "ls", args: { path: "." } },
        { eval: "take(3)" },
        { eval: 'select("name", "size")' },
    ]);
    if (pipelineResult.success) {
        console.log(`   Result: ${JSON.stringify(pipelineResult.result, null, 2)}`);
    }

    // 5. Get schema for different providers
    console.log("\n5. Available schema formats:");
    const formats = ["openai", "claude", "gemini", "llama", "mistral", "groq", "ollama"];
    for (const fmt of formats) {
        const schema = await ironshell.schema(fmt);
        if (schema.success && schema.result) {
            const modelCount = schema.result.compatible_models?.length || 0;
            console.log(`   - ${fmt}: ${modelCount} compatible models`);
        }
    }
}

// =============================================================================
// Main
// =============================================================================

async function main(): Promise<void> {
    console.log("IronShell Agent API - TypeScript Integration Examples\n");
    console.log("=".repeat(60));

    // Check if Agent API is running
    try {
        const response = await fetch("http://localhost:3002", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ action: "list_builtins" }),
        });
        if (!response.ok) {
            throw new Error("API not responding");
        }
    } catch (e) {
        console.error("Error: Agent API not running on localhost:3002");
        console.log("Start it with: ish --agent-api");
        process.exit(1);
    }

    console.log("Agent API is running!\n");

    // Run direct examples (no external API keys needed)
    await directApiExamples();

    console.log("\n" + "=".repeat(60));
    console.log("\nTo run AI integration examples:");
    console.log("  - OpenAI: Set OPENAI_API_KEY, then call openaiIntegrationExample()");
    console.log("  - Claude: Set ANTHROPIC_API_KEY, then call anthropicIntegrationExample()");
    console.log("  - Vercel AI: Set OPENAI_API_KEY, then call vercelAiIntegrationExample()");
}

// Export for use as a module
export {
    IronShellClient,
    openaiIntegrationExample,
    anthropicIntegrationExample,
    vercelAiIntegrationExample,
    directApiExamples,
};

// Run if executed directly
main().catch(console.error);
