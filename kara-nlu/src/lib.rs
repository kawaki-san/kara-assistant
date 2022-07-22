pub mod intents;
use snips_nlu_lib::SnipsNluEngine;

use crate::intents::ParsedIntent;

pub struct NLUParser {
    model: SnipsNluEngine,
}

impl NLUParser {
    pub fn new(model_path: impl AsRef<str>) -> Self {
        Self {
            model: SnipsNluEngine::from_path(model_path.as_ref()).unwrap(),
        }
    }

    pub fn parse_text(&self, text: impl AsRef<str>) {
        let result = &self.model.parse(text.as_ref(), None, None).unwrap();
        let result_json = serde_json::to_string_pretty(&result).unwrap();
        let result: ParsedIntent = serde_json::from_str(&result_json).unwrap();
        println!("{}", result_json);
        println!("{:#?}", result.intent)
    }
}
