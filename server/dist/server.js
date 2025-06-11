"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = __importDefault(require("fs"));
const http_1 = __importDefault(require("http"));
const child_process_1 = require("child_process");
const socket_io_1 = require("socket.io");
// import { Model, Recognizer } from 'vosk';
const node_fetch_1 = __importDefault(require("node-fetch"));
const cfg = JSON.parse(fs_1.default.readFileSync('config.json', 'utf-8'));
const server = http_1.default.createServer();
const io = new socket_io_1.Server(server, {
    cors: {
        origin: "*",
        methods: ["GET", "POST"]
    }
});
// const modelEn = new Model(cfg.VOSK_MODEL_PATH_EN);
// const modelHi = new Model(cfg.VOSK_MODEL_PATH_HI);
io.on('connection', (socket) => {
    console.log('Client connected');
    socket.on('command_text', async ({ room, text }) => {
        console.log('Received command:', text);
        const respText = await sendToOllama(text);
        const respAudio = await runTTS(respText);
        socket.emit('response', { room, audio: respAudio, text: respText });
    });
});
server.listen(cfg.PORT, () => console.log('Server listening on :3000'));
// function runSTT(buffer: Buffer, model: Model): Promise<string> {
// 	return new Promise((resolve, reject) => {
// 		try {
// 			const rec = new Recognizer({ model, sampleRate: cfg.SAMPLE_RATE });
// 			rec.acceptWaveform(buffer);
// 			const res = rec.finalResult();
// 			rec.free();
// 			if (!res || res.text.trim() === '') {
// 				return resolve('');
// 			}
// 			resolve(res.text);
// 		} catch (error) {
// 			reject(error);
// 		}
// 	});
// }
async function sendToOllama(prompt) {
    const systemPrompt = `You are Jarvis, a friendly, upbeat, and helpful voice assistant. You should:
1. Always respond in English, even if the user speaks in Hindi/Urdu
2. Be concise and clear in your responses
3. Maintain a warm, enthusiastic, and slightly witty personality
4. Focus on being helpful while keeping responses brief and to the point

User input: ${prompt}`;
    const res = await (0, node_fetch_1.default)(cfg.OLLAMA_API_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
            model: cfg.GEMMA_MODEL_NAME,
            prompt: systemPrompt,
            stream: false
        })
    });
    const js = (await res.json());
    return js.response.trim();
}
function runTTS(text) {
    return new Promise((resolve, reject) => {
        const modelPath = cfg.PIPER_MODEL_PATH_EN;
        const p = (0, child_process_1.spawn)(cfg.PIPER_TTS_PATH, ['--model', modelPath, '--output_file', 'out.wav']);
        p.stdin.write(text);
        p.stdin.end();
        p.on('close', () => {
            fs_1.default.readFile('out.wav', (err, data) => (err ? reject(err) : resolve(data)));
        });
        p.on('error', reject);
    });
}
