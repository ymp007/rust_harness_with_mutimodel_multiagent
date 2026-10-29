//! Provider wire adapters. Internal conversations use chat messages so tools and
//! approval policy stay identical across providers.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use y_types::AgentError;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiFormat {
    #[default]
    Openai,
    Anthropic,
    Gemini,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiEndpoint {
    pub url: String,
    #[serde(default)]
    pub format: ApiFormat,
    pub key_env: Option<String>,
}
fn err(s: &str) -> AgentError {
    AgentError::ProviderError(s.into())
}
fn arguments(call: &Value) -> Result<Value, AgentError> {
    serde_json::from_str(call["function"]["arguments"].as_str().unwrap_or("{}"))
        .map_err(|_| err("Invalid tool arguments in conversation"))
}
pub(crate) fn request(
    format: ApiFormat,
    model: &str,
    tokens: u32,
    messages: &[Value],
    tools: &[Value],
) -> Result<Value, AgentError> {
    let mut system = Vec::new();
    let mut turns: Vec<Value> = Vec::new();
    let mut names = std::collections::HashMap::new();
    for message in messages {
        let role = message["role"].as_str().unwrap_or("user");
        let text = message["content"].as_str().unwrap_or("");
        if role == "system" {
            system.push(text);
            continue;
        }
        let mut parts = Vec::new();
        if role == "tool" {
            let id = message["tool_call_id"].as_str().unwrap_or("");
            parts.push(if format == ApiFormat::Anthropic {
                json!({"type":"tool_result", "tool_use_id":id, "content":text})
            } else {
                let name = names
                    .get(id)
                    .ok_or_else(|| err("Tool result has no matching function call"))?;
                let mut result =
                    json!({"functionResponse":{"name":name,"response":{"result":text}}});
                if !id.starts_with("gemini_call_") {
                    result["functionResponse"]["id"] = json!(id);
                }
                result
            });
        } else {
            if !text.is_empty() {
                parts.push(if format == ApiFormat::Anthropic {
                    json!({"type":"text","text":text})
                } else {
                    json!({"text":text})
                });
            }
            if let Some(calls) = message["tool_calls"].as_array() {
                for call in calls {
                    let id = call["id"].as_str().unwrap_or("");
                    let name = call["function"]["name"].as_str().unwrap_or("");
                    names.insert(id.to_owned(), name.to_owned());
                    parts.push(if format == ApiFormat::Anthropic {
                        json!({"type":"tool_use","id":id,"name":name,"input":arguments(call)?})
                    } else {
                        json!({"functionCall":{"id":id,"name":name,"args":arguments(call)?}})
                    });
                }
            }
            // Gemini reasoning signatures and Anthropic opaque blocks must round
            // trip verbatim within their own protocol, never to another provider.
            let raw = if format == ApiFormat::Gemini {
                "_gemini_parts"
            } else {
                "_anthropic_content"
            };
            if role == "assistant" {
                if let Some(saved) = message[raw].as_array() {
                    parts = saved.clone();
                }
            }
        }
        if parts.is_empty() {
            continue;
        }
        let role = if role == "assistant" {
            if format == ApiFormat::Gemini {
                "model"
            } else {
                "assistant"
            }
        } else {
            "user"
        };
        let field = if format == ApiFormat::Gemini {
            "parts"
        } else {
            "content"
        };
        if let Some(last) = turns.last_mut().filter(|last| last["role"] == role) {
            last[field].as_array_mut().unwrap().extend(parts);
        } else {
            turns.push(json!({"role":role,field:parts}));
        }
    }
    let definitions:Vec<Value> = tools.iter().map(|t| {
        let f=&t["function"];
        if format == ApiFormat::Anthropic {json!({"name":f["name"],"description":f["description"],"input_schema":f["parameters"]})}
        else {json!({"name":f["name"],"description":f["description"],"parametersJsonSchema":f["parameters"]})}
    }).collect();
    if format == ApiFormat::Anthropic {
        let mut body = json!({"model":model,"max_tokens":tokens,"system":system.join("\n\n"),"messages":turns});
        if !tools.is_empty() {
            body["tools"] = json!(definitions);
        }
        Ok(body)
    } else {
        let mut body = json!({"contents":turns,"generationConfig":{"maxOutputTokens":tokens}});
        if !system.is_empty() {
            body["systemInstruction"] = json!({"parts":[{"text":system.join("\n\n")}]});
        }
        if !tools.is_empty() {
            body["tools"] = json!([{"functionDeclarations":definitions}]);
        }
        Ok(body)
    }
}
pub(crate) fn response(format: ApiFormat, value: &Value) -> Result<Value, AgentError> {
    let blocks = if format == ApiFormat::Anthropic {
        if !matches!(
            value["stop_reason"].as_str(),
            Some("end_turn" | "tool_use" | "stop_sequence")
        ) {
            return Err(err("Anthropic response incomplete, refused or truncated"));
        }
        &value["content"]
    } else {
        if value["candidates"][0]["finishReason"] != "STOP" {
            return Err(err("Gemini response incomplete, blocked or truncated"));
        }
        &value["candidates"][0]["content"]["parts"]
    };
    let blocks = blocks
        .as_array()
        .ok_or_else(|| err("Provider returned no content blocks"))?;
    let mut text = String::new();
    let mut calls = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        if b["thought"] != true {
            if let Some(s) = b["text"].as_str() {
                text.push_str(s);
            }
        }
        let call = if format == ApiFormat::Anthropic && b["type"] == "tool_use" {
            Some((b["id"].clone(), b["name"].clone(), b["input"].clone()))
        } else {
            b.get("functionCall").map(|f| {
                (
                    f.get("id")
                        .cloned()
                        .unwrap_or(json!(format!("gemini_call_{i}"))),
                    f["name"].clone(),
                    f.get("args").cloned().unwrap_or(json!({})),
                )
            })
        };
        if let Some((id, name, args)) = call {
            if !id.is_string() || !name.is_string() || !args.is_object() {
                return Err(err("Invalid provider function call"));
            }
            calls.push(json!({"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}}));
        }
    }
    if text.is_empty() && calls.is_empty() {
        return Err(err("Provider returned empty assistant content"));
    }
    let mut message = json!({"role":"assistant","content":text});
    if !calls.is_empty() {
        message["tool_calls"] = json!(calls);
    }
    message[if format == ApiFormat::Gemini {
        "_gemini_parts"
    } else {
        "_anthropic_content"
    }] = json!(blocks);
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_or_blocked_native_responses_never_execute_partial_calls() {
        assert!(response(ApiFormat::Anthropic,&json!({"stop_reason":"max_tokens","content":[{"type":"tool_use","id":"partial","name":"write_file","input":{}}]})).is_err());
        assert!(response(ApiFormat::Gemini,&json!({"candidates":[{"finishReason":"MAX_TOKENS","content":{"parts":[{"functionCall":{"name":"write_file","args":{}}}]}}]})).is_err());
        assert!(response(
            ApiFormat::Gemini,
            &json!({"promptFeedback":{"blockReason":"SAFETY"}})
        )
        .is_err());
    }
    #[test]
    fn gemini_idless_no_argument_calls_roundtrip_without_inventing_wire_ids() {
        let answer=response(ApiFormat::Gemini,&json!({"candidates":[{"finishReason":"STOP","content":{"parts":[{"functionCall":{"name":"list_files"},"thoughtSignature":"preserve-me"}]}}]})).unwrap();
        let id = answer["tool_calls"][0]["id"].clone();
        let body = request(
            ApiFormat::Gemini,
            "model",
            100,
            &[
                json!({"role":"user","content":"List files"}),
                answer,
                json!({"role":"tool","tool_call_id":id,"content":"files"}),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(
            body["contents"][1]["parts"][0]["thoughtSignature"],
            "preserve-me"
        );
        assert!(body["contents"][2]["parts"][0]["functionResponse"]
            .get("id")
            .is_none());
    }
}
