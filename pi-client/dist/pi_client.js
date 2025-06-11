"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = __importDefault(require("fs"));
const child_process_1 = require("child_process");
const socket_io_client_1 = require("socket.io-client");
const node_arecord_1 = __importDefault(require("node-arecord"));
const pocketsphinx_1 = __importDefault(require("pocketsphinx"));
const cfg = JSON.parse(fs_1.default.readFileSync('config.json', 'utf-8'));
const socket = (0, socket_io_client_1.io)(cfg.SERVER_URL);
// Initialize PocketSphinx Recognizer
const ps = new pocketsphinx_1.default.Recognizer({
    '-hmm': cfg.POCKETSPHINX_HMM_PATH,
    '-dict': cfg.POCKETSPHINX_DICT_PATH,
    '-samprate': cfg.SAMPLE_RATE,
    // You might need other options based on your PocketSphinx setup
    // '-nfft': 512,
    // '-remove_silence': false // Depends on if you want built-in silence detection
});
// Add your keyphrase search
ps.addKeyphraseSearch(cfg.POCKETSPHINX_KEYPHRASE_NAME, cfg.POCKETSPHINX_KEYPHRASE);
// Set the active search to your keyphrase search
ps.search = cfg.POCKETSPHINX_KEYPHRASE_NAME;
const recorder = new node_arecord_1.default({
    device: cfg.RECORD_DEVICE,
    rate: cfg.SAMPLE_RATE,
    channels: 1,
    format: 'S16_LE',
    type: 'raw'
});
let recording = false;
let bufferChunks = [];
let vadTimer;
socket.on('connect', () => {
    socket.emit('register_client', { room: cfg.ROOM_IDENTIFIER });
    ps.start();
    recorder.start();
});
recorder.on('data', (chunk) => {
    try {
        // Use writeSync for processing chunks as they arrive
        ps.writeSync(chunk);
    }
    catch (err) {
        console.error('PocketSphinx write error:', err);
    }
    try {
        if (recording) {
            bufferChunks.push(chunk);
            clearTimeout(vadTimer);
            vadTimer = setTimeout(stopCommand, 1000);
        }
    }
    catch (err) {
        console.error('Porcupine error:', err);
    }
});
recorder.on('error', (err) => {
    console.error('Recorder error:', err);
    recorder.stop();
    ps.stop();
});
function stopCommand() {
    recording = false;
    const audio = Buffer.concat(bufferChunks);
    socket.emit('command_audio', { room: cfg.ROOM_IDENTIFIER, audio });
}
socket.on('response_audio', (data) => {
    const play = (0, child_process_1.spawn)('aplay', [
        '-D', cfg.PLAYBACK_DEVICE,
        '-f', 'S16_LE',
        '-r', cfg.SAMPLE_RATE.toString(),
        '-c', '1'
    ]);
    play.stdin.write(data);
    play.stdin.end();
    play.on('error', e => console.error('playback error:', e));
});
process.on('SIGINT', () => {
    ps.stop();
    recorder.stop();
    process.exit();
});
ps.on('hyp', (err, hypothesis, score) => {
    if (err) {
        console.error('PocketSphinx hyp error:', err);
        return;
    }
    console.log('PocketSphinx Hypothesis:', hypothesis);
    // Check if the hypothesis matches your wake word
    // PocketSphinx might return the wake word in uppercase
    if (hypothesis && hypothesis.trim().toUpperCase() === cfg.POCKETSPHINX_KEYPHRASE.toUpperCase()) {
        if (!recording) {
            recording = true;
            bufferChunks = [];
            console.log("Wake word detected!"); // Add a log to confirm
        }
    }
});
ps.on('error', (err) => {
    console.error('PocketSphinx error event:', err);
});
