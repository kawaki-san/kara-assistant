pub enum KaraEvents {
    WakeUp(bool),
    SpeechFeed(String),
    ProcessCommand(String),
    IsBusy(bool),
}
