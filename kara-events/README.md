<h1 align="center">kara-events</h1>
<h4 align="center">This crate defines custom events that are passed to the main event loop that runs Kara. </h4>

Events may be:

- Wake word detection - when a wake word has been detected
- Incoming speech - update the text in the UI with the live transcription text
- Speech feed stopped - Generate a final transcription and use this text as a
  command
- Is Busy - Kara is currently processing a command
