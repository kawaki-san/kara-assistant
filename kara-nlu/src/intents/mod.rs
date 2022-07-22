use serde::Deserialize;

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedIntent {
    pub input: String,
    pub intent: IntentMapper,
    pub slots: Vec<Slot>,
    pub alternatives: Vec<Value>,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentMapper {
    pub intent_name: Option<Intent>,
    pub confidence_score: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub raw_value: String,
    pub value: Value,
    pub alternatives: Vec<Value>,
    pub range: Range,
    pub entity: String,
    pub slot_name: String,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Value {
    pub kind: String,
    pub value: String,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum Intent {
    #[serde(rename = "lists_createoradd")]
    ListsCreateOrAdd,
    #[serde(rename = "iot_cleaning")]
    IotCleaning,
    #[serde(rename = "iot_wemo_off")]
    IotWemoOff,
    #[serde(rename = "iot_hue_lightdim")]
    IotHueLightdim,
    #[serde(rename = "general_joke")]
    GeneralJoke,
    #[serde(rename = "play_music")]
    PlayMusic,
    #[serde(rename = "email_query")]
    EmailQuery,
    #[serde(rename = "lists_query")]
    ListsQuery,
    #[serde(rename = "general_commandstop")]
    GeneralCommandstop,
    #[serde(rename = "audio_volume_other")]
    AudioVolumeOther,
    #[serde(rename = "weather_query")]
    WeatherQuery,
    #[serde(rename = "datetime_query")]
    DateTimeQuery,
    #[serde(rename = "transport_taxi")]
    TransportTaxi,
    #[serde(rename = "general_confirm")]
    GeneralConfirm,
    #[serde(rename = "general_praise")]
    GeneralPraise,
    #[serde(rename = "audio_volume_mute")]
    AudioVolumeMute,
    #[serde(rename = "iot_hue_lightoff")]
    IotHueLightoff,
    #[serde(rename = "music_likeness")]
    MusicLikeness,
    #[serde(rename = "recommendation_locations")]
    RecommendationLocations,
    #[serde(rename = "cooking_query")]
    CookingQuery,
    #[serde(rename = "general_dontcare")]
    GeneralDontCare,
    #[serde(rename = "audio_volume_up")]
    AudioVolumeUp,
    #[serde(rename = "play_radio")]
    PlayRadio,
    #[serde(rename = "qa_definition")]
    QADefinition,
    #[serde(rename = "lists_remove")]
    ListsRemove,
    #[serde(rename = "calendar_remove")]
    CalendarRemove,
    #[serde(rename = "audio_volume_down")]
    AudioVolumeDown,
    #[serde(rename = "qa_factoid")]
    QAFactoid,
    #[serde(rename = "transport_traffic")]
    TransportTraffic,
    #[serde(rename = "iot_wemo_on")]
    IotWemoOn,
    #[serde(rename = "iot_coffee")]
    IotCoffee,
    #[serde(rename = "music_settings")]
    MusicSettings,
    #[serde(rename = "datetime_convert")]
    DatetimeConvert,
    #[serde(rename = "social_query")]
    SocialQuery,
    #[serde(rename = "calendar_query")]
    CalendarQuery,
    #[serde(rename = "cooking_recipe")]
    CookingRecipe,
    #[serde(rename = "transport_ticket")]
    TransportTicket,
    #[serde(rename = "iot_hue_lighton")]
    IotHueLightOn,
    #[serde(rename = "music_dislikeness")]
    MusicDislikeness,
    #[serde(rename = "general_repeat")]
    GeneralRepeat,
    #[serde(rename = "transport_query")]
    TransportQuery,
    #[serde(rename = "music_query")]
    MusicQuery,
    #[serde(rename = "general_quirky")]
    GeneralQuirky,
    #[serde(rename = "qa_currency")]
    QACurrency,
    #[serde(rename = "iot_hue_lightchange")]
    IotHueLightChange,
    #[serde(rename = "iot_hue_lightup")]
    IotHueLightUp,
    #[serde(rename = "email_querycontact")]
    EmailQueryContact,
    #[serde(rename = "email_addcontact")]
    EmailAddContact,
    #[serde(rename = "general_explain")]
    GeneralExplain,
    #[serde(rename = "recommendation_events")]
    RecommendationEvents,
    #[serde(rename = "alarm_set")]
    AlarmSet,
    #[serde(rename = "play_game")]
    PlayGame,
    #[serde(rename = "play_podcasts")]
    PlayPodcasts,
    #[serde(rename = "takeaway_query")]
    TakeawayQuery,
    #[serde(rename = "general_greet")]
    GeneralGreet,
    #[serde(rename = "calendar_set")]
    CalendarSet,
    #[serde(rename = "alarm_query")]
    AlarmQuery,
    #[serde(rename = "qa_maths")]
    QAMaths,
    #[serde(rename = "takeaway_order")]
    TakeawayOrder,
    #[serde(rename = "email_sendemail")]
    EmailSend,
    #[serde(rename = "general_negate")]
    GeneralNegate,
    #[serde(rename = "alarm_remove")]
    AlarmRemove,
    #[serde(rename = "news_query")]
    NewsQuery,
    #[serde(rename = "recommendation_movies")]
    RecommendationMovies,
    #[serde(rename = "social_post")]
    SocialPost,
    #[serde(rename = "play_audiobook")]
    PlayAudiobook,
    #[serde(rename = "general_affirm")]
    GeneralAffirm,
    #[serde(rename = "qa_stock")]
    QAStock,
}
