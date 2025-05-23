import fs from 'fs';
import { spawn, ChildProcessWithoutNullStreams } from 'child_process';
import { io, Socket } from 'socket.io-client';
import { Porcupine, PorcupineOptions } from '@picovoice/porcupine-node';
import AudioRecorder from 'node-arecord';

interface ClientConfig {
  PORCUPINE_ACCESS_KEY: string;
  WAKE_WORD: string;
  SERVER_URL: string;
  ROOM_IDENTIFIER: string;
  RECORD_DEVICE: string;
  PLAYBACK_DEVICE: string;
  SAMPLE_RATE: number;
}

const cfg: ClientConfig = JSON.parse(fs.readFileSync('config.json', 'utf-8'));
const socket: Socket = io(cfg.SERVER_URL);

const porcupine = new Porcupine(
  cfg.PORCUPINE_ACCESS_KEY,
  [cfg.WAKE_WORD] as PorcupineOptions['keywordPaths']
);

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
  recorder.start();
});

recorder.on('data', (chunk: Buffer) => {
  // Wake-word detection
  const pcm = new Int16Array(chunk.buffer);
  try {
    if (!recording && porcupine.process(pcm) >= 0) {
      recording = true;
      bufferChunks = [];
    }
    if (recording) {
      bufferChunks.push(chunk);
      clearTimeout(vadTimer);
      vadTimer = setTimeout(stopCommand, 1000);
    }
  } catch (err) {
    console.error('Porcupine error:', err);
  }
});

recorder.on('error', err => {
  console.error('Recorder error:', err);
  recorder.stop();
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
  porcupine.release();
  recorder.stop();
  process.exit();
});
