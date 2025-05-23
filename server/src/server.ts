import fs from 'fs';
import http from 'http';
import { spawn } from 'child_process';
import { Server, Socket } from 'socket.io';
import { Model, Recognizer } from 'vosk';
import fetch from 'node-fetch';

interface ServerConfig {
	VOSK_MODEL_PATH_EN: string;
	VOSK_MODEL_PATH_HI: string;
	PIPER_TTS_PATH: string;
	PIPER_MODEL_PATH_EN: string;
	PIPER_MODEL_PATH_HI: string;
	OLLAMA_API_URL: string;
	GEMMA_MODEL_NAME: string;
	SAMPLE_RATE: number;
}

const cfg: ServerConfig = JSON.parse(fs.readFileSync('config.json', 'utf-8'));
const server = http.createServer();
const io = new Server(server);

const modelEn = new Model(cfg.VOSK_MODEL_PATH_EN);
const modelHi = new Model(cfg.VOSK_MODEL_PATH_HI);

io.on('connection', (socket: Socket) => {
	socket.on('command_audio', async ({ room, audio }: { room: string; audio: Buffer }) => {
		try {
			const [textEn, textHi] = await Promise.all([
				runSTT(audio, modelEn),
				runSTT(audio, modelHi)
			]);
			const command = textHi.length > textEn.length ? textHi : textEn;

			const respText = await sendToOllama(command);
			const lang = command === textHi ? 'hi' : 'en';
			const respAudio = await runTTS(respText, lang);

			socket.emit('response_audio', respAudio);
		} catch (err) {
			console.error('Processing error:', err);
		}
	});
});

server.listen(3000, () => console.log('Server listening on :3000'));

function runSTT(buffer: Buffer, model: Model): Promise<string> {
	return new Promise(resolve => {
		const rec = new Recognizer({ model, sampleRate: cfg.SAMPLE_RATE });
		rec.acceptWaveform(buffer);
		const res = rec.finalResult();
		rec.free();
		const parsed = JSON.parse(res) as { text: string };
		resolve(parsed.text);
	});
}

async function sendToOllama(prompt: string): Promise<string> {
	const res = await fetch(cfg.OLLAMA_API_URL, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ model: cfg.GEMMA_MODEL_NAME, prompt, stream: false })
	});
	const js = (await res.json()) as { response: string };
	return js.response.trim();
}

function runTTS(text: string, lang: 'en' | 'hi'): Promise<Buffer> {
	return new Promise((resolve, reject) => {
		const modelPath = lang === 'hi' ? cfg.PIPER_MODEL_PATH_HI : cfg.PIPER_MODEL_PATH_EN;
		const p = spawn(cfg.PIPER_TTS_PATH, ['--model', modelPath, '--output_file', 'out.wav']);
		p.stdin.write(text);
		p.stdin.end();
		p.on('close', () => {
			fs.readFile('out.wav', (err, data) => (err ? reject(err) : resolve(data)));
		});
		p.on('error', reject);
	});
}
