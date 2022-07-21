use snips_nlu_lib::SnipsNluEngine;

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
        println!("{}", result_json);
    }
}
