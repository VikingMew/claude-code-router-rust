use ccr_router::count_tokens;
use ccr_types::{Message, MessagesRequest, TokenizerBackend};
use eframe::egui;
use serde_json::json;

pub struct TokenCounterTab {
    model: String,
    messages_json: String,
    result: String,
    error: String,
    tokenizer_backend: TokenizerBackend,
}

impl TokenCounterTab {
    pub fn new() -> Self {
        Self {
            model: "claude-3-5-sonnet-20241022".to_string(),
            messages_json: r#"[
  {
    "role": "user",
    "content": "Hello"
  }
]"#
            .to_string(),
            result: String::new(),
            error: String::new(),
            tokenizer_backend: TokenizerBackend::Tiktoken,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Token Counter");
            if ui.button("Calculate Tokens").clicked() {
                self.calculate();
            }
        });

        ui.horizontal(|ui| {
            ui.label("Model:");
            ui.text_edit_singleline(&mut self.model);
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Tokenizer:");
            ui.radio_value(
                &mut self.tokenizer_backend,
                TokenizerBackend::Tiktoken,
                "tiktoken",
            );
            ui.radio_value(
                &mut self.tokenizer_backend,
                TokenizerBackend::Huggingface,
                "huggingface",
            );
        });

        ui.add_space(8.0);

        ui.label("Messages (JSON):");
        ui.add(
            egui::TextEdit::multiline(&mut self.messages_json)
                .desired_rows(10)
                .desired_width(f32::INFINITY)
                .code_editor(),
        );

        ui.add_space(8.0);

        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::RED, &self.error);
        }

        if !self.result.is_empty() {
            ui.colored_label(egui::Color32::GREEN, &self.result);
        }

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("Example: Simple").clicked() {
                self.load_simple_example();
            }
            if ui.button("Example: Conversation").clicked() {
                self.load_conversation_example();
            }
            if ui.button("Example: Complex Content").clicked() {
                self.load_complex_example();
            }
        });
    }

    fn calculate(&mut self) {
        self.error.clear();
        self.result.clear();

        // Parse JSON
        let messages: Result<Vec<serde_json::Value>, _> = serde_json::from_str(&self.messages_json);
        let messages = match messages {
            Ok(m) => m,
            Err(e) => {
                self.error = format!("JSON parse error: {}", e);
                return;
            }
        };

        // Convert to Message structs
        let mut parsed_messages = Vec::new();
        for msg in messages {
            let role = msg
                .get("role")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            let content = msg.get("content").cloned().unwrap_or(json!(""));

            parsed_messages.push(Message { role, content });
        }

        // Build MessagesRequest
        let request = MessagesRequest {
            model: self.model.clone(),
            messages: parsed_messages,
            max_tokens: Some(8192),
            stream: false,
            system: None,
            tools: None,
            thinking: None,
        };

        // Count tokens
        let count = count_tokens(&request, &self.tokenizer_backend);
        self.result = format!("✓ {} tokens", count);
    }

    fn load_simple_example(&mut self) {
        self.messages_json = r#"[
  {
    "role": "user",
    "content": "Hello"
  }
]"#
        .to_string();
    }

    fn load_conversation_example(&mut self) {
        self.messages_json = r#"[
  {
    "role": "user",
    "content": "What is Rust?"
  },
  {
    "role": "assistant",
    "content": "Rust is a systems programming language..."
  },
  {
    "role": "user",
    "content": "Tell me more about ownership"
  }
]"#
        .to_string();
    }

    fn load_complex_example(&mut self) {
        self.messages_json = r#"[
  {
    "role": "user",
    "content": [
      {
        "type": "text",
        "text": "Analyze this"
      }
    ]
  }
]"#
        .to_string();
    }
}
