<h1 align="center">kara-audio</h1>
<h4 align="center">TLDR: This crate handles audio processesing and visualisation </h4>

## Key responsibilities

- Records audio and prepares it for visualising (FFT)
- Converts the input audio to a format usable for transcription (`mono@16Khz`)
- Listens for utterances and wakewords and sends [`kara-events`](../kara-events)
  accordingly
