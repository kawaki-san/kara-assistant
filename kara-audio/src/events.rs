pub enum KaraEvents {
    WakeWordDetected(bool),
    SpeechFeed(String),
    ProcessCommand(String),
}
