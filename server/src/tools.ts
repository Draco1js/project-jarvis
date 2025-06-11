import fetch from 'node-fetch';
import { ToolDefinition, ToolFunction, WebSearchArgs, CalculatorArgs, DuckDuckGoResponse } from './types';

export const webSearchTool: ToolDefinition = {
	type: 'function',
	function: {
		name: 'webSearch',
		description: 'Search the web for information using DuckDuckGo',
		parameters: {
			type: 'object',
			properties: {
				query: {
					type: 'string',
					description: 'The search query'
				}
			},
			required: ['query']
		}
	}
};

export const calculatorTool: ToolDefinition = {
	type: 'function',
	function: {
		name: 'calculator',
		description: 'Perform basic arithmetic operations',
		parameters: {
			type: 'object',
			properties: {
				operation: {
					type: 'string',
					enum: ['add', 'subtract', 'multiply', 'divide'],
					description: 'The arithmetic operation to perform'
				},
				a: {
					type: 'number',
					description: 'First number'
				},
				b: {
					type: 'number',
					description: 'Second number'
				}
			},
			required: ['operation', 'a', 'b']
		}
	}
};

export const webSearch: ToolFunction<WebSearchArgs> = async (args: WebSearchArgs): Promise<string> => {
	try {
		const url = `https://api.duckduckgo.com/?q=${encodeURIComponent(args.query)}&format=json&no_redirect=1&no_html=1`;
		const res = await fetch(url);
		const data = await res.json() as DuckDuckGoResponse;
		
		let response = '';
		if (data.Answer) {
			response = data.Answer;
		} else if (data.AbstractText) {
			response = data.AbstractText;
			if (data.AbstractSource) {
				response += ` (Source: ${data.AbstractSource})`;
			}
		} else if (data.RelatedTopics && data.RelatedTopics.length > 0) {
			response = data.RelatedTopics[0].Text;
		}
		
		if (!response) {
			return "I couldn't find specific information about that. Could you please rephrase your question?";
		}
		
		return response;
	} catch (error) {
		console.error('Error performing web search:', error);
		return "I'm having trouble searching the web right now. Please try again in a moment.";
	}
};

export const calculator: ToolFunction<CalculatorArgs> = async (args: CalculatorArgs): Promise<string> => {
	try {
		const { operation, a, b } = args;
		let result: number;
		
		switch (operation) {
			case 'add':
				result = a + b;
				break;
			case 'subtract':
				result = a - b;
				break;
			case 'multiply':
				result = a * b;
				break;
			case 'divide':
				if (b === 0) {
					return "Error: Division by zero is not allowed.";
				}
				result = a / b;
				break;
			default:
				return `Error: Unsupported operation '${operation}'`;
		}
		
		return result.toString();
	} catch (error) {
		console.error('Error performing calculation:', error);
		return "I'm having trouble performing the calculation. Please try again.";
	}
};

export const availableFunctions: Record<string, ToolFunction<any>> = {
	webSearch,
	calculator
}; 