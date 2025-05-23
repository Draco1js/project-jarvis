Below is a **ready-to-go, GitHub-style project** you can clone, configure, and deploy across your Pis and your home-lab server. It includes:

* **`pi-client/`**: Raspberry Pi client code
* **`server/`**: Central Node.js server code
* **`ansible/`**: Inventory & playbook to update your Pis
* **`README.md`**: Setup & deployment instructions

---

## 📂 Project Structure

```
voice-assistant/
├── README.md
├── pi-client/
│   ├── package.json
│   ├── pi_client.js
│   └── config.json
├── server/
│   ├── package.json
│   ├── server.js
│   └── config.json
└── ansible/
    ├── inventory.ini
    └── update.yml
```

---

## 📘 README.md

````markdown
# Offline Multilingual Voice Assistant

## Overview

This repo contains:

- **pi-client**: Node.js app for each Raspberry Pi Zero 2 W (or Pi 3/4).  
  - Wake-word detection (Porcupine)  
  - Audio capture (arecord) & playback (aplay)  
  - Streams command audio to central server  

- **server**: Node.js app on your home-lab machine.  
  - STT (Vosk, English/Hindi)  
  - Text generation (Gemma via Ollama)  
  - TTS (Piper)  
  - Returns response audio back to Pi  

- **ansible**: Automates pulling updates and restarting the client on each Pi.

## Prerequisites

### On All Pis

- Raspberry Pi OS Lite  
- Node.js ≥v18  
- `arecord`/`aplay` (via `alsa-utils`)  
- Porcupine Node.js SDK  
- SSH keys set up for your control machine

### On Server

- Ubuntu/Debian (or any Linux)  
- Node.js ≥v18  
- Python & `pip install vosk piper-tts`  
- Ollama (with `gemma:latest` model pulled)  
- Ansible  

## 1. Clone Repo

```bash
git clone https://github.com/you/voice-assistant.git
cd voice-assistant
````

## 2. Configure

### `pi-client/config.json`

```json
{
  "PORCUPINE_ACCESS_KEY": "YOUR_PORCUPINE_ACCESS_KEY",
  "WAKE_WORD": "picovoice",
  "SERVER_URL": "http://YOUR_SERVER_IP:3000",
  "ROOM_IDENTIFIER": "living_room",
  "RECORD_DEVICE": "hw:1,0",
  "PLAYBACK_DEVICE": "hw:0,0",
  "SAMPLE_RATE": 16000
}
```

### `server/config.json`

```json
{
  "VOSK_MODEL_PATH_EN": "/opt/models/vosk-model-small-en-us-0.15",
  "VOSK_MODEL_PATH_HI": "/opt/models/vosk-model-small-hi-0.22",
  "PIPER_TTS_PATH": "/usr/local/bin/piper",
  "PIPER_MODEL_PATH_EN": "/opt/models/en_US-standard-medium.onnx",
  "PIPER_MODEL_PATH_HI": "/opt/models/hi_IN-indictts-male.onnx",
  "OLLAMA_API_URL": "http://localhost:11434/api/generate",
  "GEMMA_MODEL_NAME": "gemma:latest",
  "SAMPLE_RATE": 16000
}
```

## 3. Install & Run

### Pi Client

```bash
cd pi-client
npm install
pm2 start pi_client.js --name voice-client
pm2 save
```

### Server

```bash
cd server
npm install
pm2 start server.js --name voice-server
pm2 save
```

## 4. Automate Updates with Ansible

### `ansible/inventory.ini`

```ini
[pis]
living-room ansible_host=192.168.1.101 ansible_user=pi
kitchen     ansible_host=192.168.1.102 ansible_user=pi
```

### `ansible/update.yml`

```yaml
- name: Update Voice Assistant on all Pis
  hosts: pis
  gather_facts: no

  tasks:
    - name: Pull latest code
      git:
        repo: 'https://github.com/you/voice-assistant.git'
        dest: '/home/pi/voice-assistant'
        version: main

    - name: Install Node.js deps
      npm:
        path: '/home/pi/voice-assistant/pi-client'
        production: yes

    - name: Restart client
      shell: |
        pm2 restart voice-client || pm2 start /home/pi/voice-assistant/pi-client/pi_client.js --name voice-client
```

Run it:

```bash
cd ansible
ansible-playbook -i inventory.ini update.yml
```

---

## 🛠️ Troubleshooting

* **Porcupine errors**: Verify your AccessKey and device ID in `config.json`.
* **Vosk model not found**: Ensure models are unzipped at the paths in `server/config.json`.
* **Network issues**: Ping your server from each Pi and confirm port 3000 is open.
* **Permission errors**: Make sure your Pi user can run `arecord`/`aplay` (add to `audio` group).

---

Enjoy your fully-offline, multilingual Jarvis-style assistant!