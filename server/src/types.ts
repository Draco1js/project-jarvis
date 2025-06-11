export interface ServerConfig {
	VOSK_MODEL_PATH_EN: string;
	VOSK_MODEL_PATH_HI: string;
	PIPER_TTS_PATH: string;
	PIPER_MODEL_PATH_EN: string;
	OLLAMA_API_URL: string;
	GEMMA_MODEL_NAME: string;
	SAMPLE_RATE: number;
	PORT: number;
}

export interface OllamaChatMessage {
	role: 'system' | 'user' | 'assistant' | 'tool';
	content: string;
	tool_calls?: Array<{
		function: {
			name: string;
			arguments: { [key: string]: any };
		};
	}>;
}

export interface ChatHistoryMessage {
	text: string;
	timestamp: number;
	room: string;
	isUser: boolean;
}

export interface ChatContext {
	messages: ChatHistoryMessage[];
	conversationHistory: string;
}

export interface DuckDuckGoResponse {
	AbstractText?: string;
	Answer?: string;
	RelatedTopics?: Array<{ Text: string }>;
	AbstractSource?: string;
	AbstractURL?: string;
}

export interface WebSearchArgs {
	query: string;
}

export interface CalculatorArgs {
	operation: string;
	a: number;
	b: number;
}

export type ToolArgs = WebSearchArgs | CalculatorArgs;

export interface ToolFunction<T extends ToolArgs> {
	(args: T): Promise<string>;
}

export interface ToolDefinition {
	type: 'function';
	function: {
		name: string;
		description: string;
		parameters: {
			type: 'object';
			properties: Record<string, {
				type: string;
				description: string;
				enum?: string[];
			}>;
			required: string[];
		};
	};
} 