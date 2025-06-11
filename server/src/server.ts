import fs from 'fs';
import http from 'http';
import { spawn } from 'child_process';
import { Server, Socket } from 'socket.io';
import { Ollama } from 'ollama';
import { 
    ServerConfig, 
    ChatContext, 
    ChatHistoryMessage, 
    OllamaChatMessage,
    ToolArgs 
} from './types';
import { 
    webSearchTool, 
    calculatorTool, 
    availableFunctions 
} from './tools';

const cfg: ServerConfig = JSON.parse(fs.readFileSync('config.json', 'utf-8'));
const server = http.createServer();
const io = new Server(server, {
	cors: {
		origin: "*",  // Allow all origins in development
		methods: ["GET", "POST"]
	}
});

// Store chat history and context for each room
const chatContexts: { [room: string]: ChatContext } = {};

// Initialize Ollama client with default host
const ollama = new Ollama({ 
	host: cfg.OLLAMA_API_URL  // Default Ollama host
});

// Check if Ollama is running and model is available
async function checkOllamaSetup() {
	try {
		const models = await ollama.list();
		const modelExists = models.models.some(m => m.name === cfg.GEMMA_MODEL_NAME);
		
		if (!modelExists) {
			console.log(`Model ${cfg.GEMMA_MODEL_NAME} not found, attempting to pull...`);
			let currentDigestDone = false;
			const stream = await ollama.pull({ model: cfg.GEMMA_MODEL_NAME, stream: true });
			
			for await (const part of stream) {
				if (part.digest) {
					let percent = 0;
					if (part.completed && part.total) {
						percent = Math.round((part.completed / part.total) * 100);
					}
					process.stdout.clearLine(0); // Clear the current line
					process.stdout.cursorTo(0); // Move cursor to the beginning of the line
					process.stdout.write(`${part.status} ${percent}%...`); // Write the new text
					if (percent === 100 && !currentDigestDone) {
						console.log(); // Output to a new line
						currentDigestDone = true;
					} else {
						currentDigestDone = false;
					}
				} else {
					console.log(part.status);
				}
			}
			console.log(`Model ${cfg.GEMMA_MODEL_NAME} pulled successfully`);
		}
	} catch (error) {
		console.error('Error checking Ollama setup:', error);
		throw new Error('Failed to connect to Ollama. Please make sure Ollama is running on http://localhost:11434');
	}
}

// Run setup check when server starts
checkOllamaSetup().catch(error => {
	console.error('Ollama setup failed:', error);
	process.exit(1);
});

// Cleanup function to remove old messages and contexts
function cleanupOldMessages() {
	const MAX_MESSAGES = 50; // Maximum messages to keep per room
	const MAX_CONTEXT_LENGTH = 2000; // Maximum context length in characters
	
	for (const room in chatContexts) {
		const context = chatContexts[room];
		
		// Remove old messages if we have too many
		if (context.messages.length > MAX_MESSAGES) {
			context.messages = context.messages.slice(-MAX_MESSAGES);
		}
		
		// Truncate conversation history if it's too long
		if (context.conversationHistory.length > MAX_CONTEXT_LENGTH) {
			// Keep the most recent messages
			const messages = context.conversationHistory.split('\n');
			const recentMessages = messages.slice(-10); // Keep last 10 messages
			context.conversationHistory = recentMessages.join('\n');
		}
	}
}

// Run cleanup every hour
const cleanupInterval = setInterval(cleanupOldMessages, 60 * 60 * 1000);

io.on('connection', (socket: Socket) => {
	console.log('Client connected');

	// Send chat history when a client connects
	socket.on('join_room', (room: string) => {
		socket.join(room);
		if (!chatContexts[room]) {
			chatContexts[room] = {
				messages: [],
				conversationHistory: ''
			};
		}
		socket.emit('chat_history', chatContexts[room].messages);
	});

	socket.on('command_text', async ({ room, text }: { room: string; text: string }) => {
		console.log('Received command:', text);
		try {
			// Store user message
			const userMessage: ChatHistoryMessage = {
				text,
				timestamp: Date.now(),
				room,
				isUser: true
			};
			
			if (!chatContexts[room]) {
				chatContexts[room] = {
					messages: [],
					conversationHistory: ''
				};
			}
			chatContexts[room].messages.push(userMessage);
			
			// Update conversation history
			chatContexts[room].conversationHistory += `User: ${text}\n`;

			// Send the message to Ollama and handle streaming
			await sendToOllama(text, chatContexts[room].conversationHistory, socket, room);

		} catch (error) {
			console.error('Error processing command:', error);
			const errorMessage: ChatHistoryMessage = {
				text: 'Sorry, I encountered an error processing your request.',
				timestamp: Date.now(),
				room,
				isUser: false
			};
			chatContexts[room].messages.push(errorMessage);
			
			// Send error message using streaming events
			socket.emit('response_chunk', {
				room,
				text: errorMessage.text,
				timestamp: errorMessage.timestamp
			});
			
			socket.emit('response_complete', {
				room,
				timestamp: errorMessage.timestamp
			});
		}
	});

	// Handle disconnection
	socket.on('disconnect', () => {
		console.log('Client disconnected');
	});
});

// Handle server shutdown
process.on('SIGINT', () => {
	console.log('Shutting down server...');
	ollama.abort();
	clearInterval(cleanupInterval);
	io.close(() => {
		console.log('Socket.IO server closed');
		process.exit(0);
	});
});

process.on('SIGTERM', () => {
	console.log('Shutting down server...');
	ollama.abort();
	clearInterval(cleanupInterval);
	io.close(() => {
		console.log('Socket.IO server closed');
		process.exit(0);
	});
});

server.listen(cfg.PORT, () => console.log('Server listening on :3000'));

async function sendToOllama(prompt: string, conversationHistory: string, socket: Socket, room: string): Promise<void> {
	const systemPrompt = `You are Jarvis, a friendly, upbeat, and helpful voice assistant. Please sound as human as possible and keep the conversation as natural as possible. Be helpful when needed. Previous conversation: ${conversationHistory} User input: ${prompt}`;

	try {
		console.log('Sending message to Ollama...');
		const messages: OllamaChatMessage[] = [
			{ role: 'system', content: systemPrompt },
			{ role: 'user', content: prompt }
		];

		// First API call: Send the query and function descriptions to the model
		const response = await ollama.chat({
			model: cfg.GEMMA_MODEL_NAME,
			messages: messages,
			// tools: [webSearchTool, calculatorTool],
			stream: true
		});

		console.log('Got streaming response from Ollama');
		let fullResponse = '';
		let needsWebSearch = false;
		let hasToolCall = false;

		// Create a message container for the response
		const responseMessage: ChatHistoryMessage = {
			text: '',
			timestamp: Date.now(),
			room,
			isUser: false
		};

		for await (const chunk of response) {
			console.log('Received chunk:', JSON.stringify(chunk));
			
			// Handle content updates
			if (chunk.message?.content) {
				const content = chunk.message.content;
				fullResponse += content;
				responseMessage.text = fullResponse;
				
				console.log('Sending chunk to client:', content);
				socket.emit('response_chunk', {
					room,
					text: content,
					timestamp: Date.now()
				});
			}

			// Handle tool calls
			if (chunk.message?.tool_calls && chunk.message.tool_calls.length > 0) {
				console.log('Processing tool calls:', JSON.stringify(chunk.message.tool_calls));
				hasToolCall = true;
				
				for (const tool of chunk.message.tool_calls) {
					const functionName = tool.function.name as keyof typeof availableFunctions;
					const functionToCall = availableFunctions[functionName];
					if (functionToCall) {
						const args = JSON.parse(JSON.stringify(tool.function.arguments)) as ToolArgs;
						
						// If it's a web search, we need to get a new response
						if (functionName === 'webSearch') {
							needsWebSearch = true;
							// Notify client that we're searching
							socket.emit('status_update', { 
								room,
								status: 'searching',
								timestamp: Date.now()
							});
							
							const functionResponse = await functionToCall(args);
							
							// Notify client that search is complete
							socket.emit('status_update', { 
								room,
								status: 'complete',
								timestamp: Date.now()
							});

							// Get final response with search results
							const finalResponse = await ollama.chat({
								model: cfg.GEMMA_MODEL_NAME,
								messages: [
									...messages,
									{ role: 'assistant', content: '', tool_calls: [{ function: { name: 'webSearch', arguments: args } }] },
									{ role: 'tool', content: functionResponse }
								],
								stream: true
							});

							fullResponse = '';
							for await (const finalChunk of finalResponse) {
								console.log('Received final chunk:', JSON.stringify(finalChunk));
								if (finalChunk.message?.content) {
									const content = finalChunk.message.content;
									fullResponse += content;
									responseMessage.text = fullResponse;
									
									console.log('Sending final chunk to client:', content);
									socket.emit('response_chunk', {
										room,
										text: content,
										timestamp: Date.now()
									});
								}
							}
							break;
						} else {
							// Handle other tools (like calculator)
							const functionResponse = await functionToCall(args);
							messages.push({
								role: 'assistant',
								content: '',
								tool_calls: [{ function: { name: functionName, arguments: args } }]
							});
							messages.push({
								role: 'tool',
								content: functionResponse
							});
						}
					}
				}
			}
		}

		// If we didn't need any tools, update the final response
		if (!hasToolCall) {
			responseMessage.text = fullResponse;
			responseMessage.timestamp = Date.now();
		}

		// Store in chat history
		chatContexts[room].messages.push(responseMessage);
		chatContexts[room].conversationHistory += `Assistant: ${fullResponse}\n`;

		// Send completion event
		console.log('Sending completion event');
		socket.emit('response_complete', {
			room,
			timestamp: responseMessage.timestamp
		});

	} catch (error) {
		console.error('Error in sendToOllama:', error);
		throw error;
	}
}

function runTTS(text: string): Promise<Buffer> {
	return new Promise((resolve, reject) => {
		try {
			const modelPath = cfg.PIPER_MODEL_PATH_EN;
			const p = spawn(cfg.PIPER_TTS_PATH, ['--model', modelPath, '--output_file', 'out.wav']);
			p.stdin.write(text);
			p.stdin.end();
			p.on('close', () => {
				fs.readFile('out.wav', (err, data) => (err ? reject(err) : resolve(data)));
			});
			p.on('error', reject);
		} catch (error) {
			reject(error);
		}
	});
} 