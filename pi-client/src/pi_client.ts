import fs from 'fs';
import { spawn, ChildProcessWithoutNullStreams } from 'child_process';
import { io, Socket } from 'socket.io-client';
import AudioRecorder from 'node-arecord';
import PocketSphinx from 'pocketsphinx';

interface ClientConfig {
  PORCUPINE_ACCESS_KEY: string;
  SERVER_URL: string;
  ROOM_IDENTIFIER: string;
  RECORD_DEVICE: string;
  PLAYBACK_DEVICE: string;
  SAMPLE_RATE: number;
  // Add PocketSphinx configuration
  POCKETSPHINX_HMM_PATH: string; // Path to your HMM model
  POCKETSPHINX_DICT_PATH: string; // Path to your dictionary file (needed for general recognition but helpful for keyphrase)
  POCKETSPHINX_KEYPHRASE_NAME: string; // A name for your keyphrase search
  POCKETSPHINX_KEYPHRASE: string; // Your actual wake word
  // Note: The keyphrase threshold might be set differently in this binding
}

const cfg: ClientConfig = JSON.parse(fs.readFileSync('config.json', 'utf-8'));
const socket: Socket = io(cfg.SERVER_URL);

// Initialize PocketSphinx Recognizer
const ps = new PocketSphinx.Recognizer({
  '-hmm': cfg.POCKETSPHINX_HMM_PATH,
  '-dict': cfg.POCKETSPHINX_DICT_PATH, // Include dictionary even for keyphrase for robustness
  '-samprate': cfg.SAMPLE_RATE,
  // You might need other options based on your PocketSphinx setup
  // '-nfft': 512,
  // '-remove_silence': false // Depends on if you want built-in silence detection
});

// Add your keyphrase search
ps.addKeyphraseSearch(cfg.POCKETSPHINX_KEYPHRASE_NAME, cfg.POCKETSPHINX_KEYPHRASE);

// Set the active search to your keyphrase search
ps.search = cfg.POCKETSPHINX_KEYPHRASE_NAME;

const recorder = new AudioRecorder({
  device: cfg.RECORD_DEVICE,
  rate: cfg.SAMPLE_RATE,
  channels: 1,
  format: 'S16_LE',
  type: 'raw'
});

let recording = false;
let bufferChunks: Buffer[] = [];
let vadTimer: NodeJS.Timeout;

socket.on('connect', () => {
  socket.emit('register_client', { room: cfg.ROOM_IDENTIFIER });
  ps.start();
  recorder.start();
});

recorder.on('data', (chunk: Buffer) => {
  try {
    // Use writeSync for processing chunks as they arrive
    ps.writeSync(chunk);
  } catch (err) {
    console.error('PocketSphinx write error:', err);
  }
  try {
    if (recording) {
      bufferChunks.push(chunk);
      clearTimeout(vadTimer);
      vadTimer = setTimeout(stopCommand, 1000);
    }
  } catch (err) {
    console.error('Porcupine error:', err);
  }
});

recorder.on('error', (err: Error) => {
  console.error('Recorder error:', err);
  recorder.stop();
  ps.stop();
});

function stopCommand(): void {
  recording = false;
  const audio = Buffer.concat(bufferChunks);
  socket.emit('command_audio', { room: cfg.ROOM_IDENTIFIER, audio });
}

socket.on('response_audio', (data: Buffer) => {
  const play: ChildProcessWithoutNullStreams = spawn('aplay', [
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

ps.on('hyp', (err: Error | null, hypothesis: string | null, score: number) => {
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

ps.on('error', (err: Error) => {
  console.error('PocketSphinx error event:', err);
});
