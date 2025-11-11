use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterSchema {
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub url: String,
    pub method: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub backend: String, // "cli" (default) or "http"
    #[serde(default)]
    pub http: Option<HttpConfig>,
    pub parameters: HashMap<String, ParameterSchema>,
}

#[derive(Debug, Deserialize)]
struct ToolsFile {
    tools: Vec<ToolConfig>,
}

/// Load tools from ~/.config/transcribe-rs/tools.json
pub fn load_tools() -> Result<Vec<ToolConfig>, Box<dyn Error>> {
    let config_path = get_config_path()?;

    if !config_path.exists() {
        log(&format!("Tools config not found at {:?}, returning empty list", config_path));
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&config_path)?;
    let tools_file: ToolsFile = serde_json::from_str(&contents)?;

    log(&format!("Loaded {} tools from config", tools_file.tools.len()));
    Ok(tools_file.tools)
}

/// Execute a tool with parameter substitution
pub fn execute_tool(tool: &ToolConfig, params: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    let backend = if tool.backend.is_empty() { "cli" } else { &tool.backend };

    match backend {
        "cli" => execute_cli_tool(tool, params),
        "http" => execute_http_tool(tool, params),
        _ => Err(format!("Unknown backend: {}", backend).into()),
    }
}

fn execute_cli_tool(tool: &ToolConfig, params: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    // Substitute parameters in command and args
    let mut substituted_args = Vec::new();

    for arg_template in &tool.args {
        let substituted = substitute_params(arg_template, params)?;
        substituted_args.push(substituted);
    }

    log(&format!("Executing CLI tool: {} {:?}", tool.command, substituted_args));

    // Use spawn() instead of output() to launch detached without waiting
    let child = Command::new(&tool.command)
        .args(&substituted_args)
        .spawn()?;

    let pid = child.id();
    log(&format!("Tool launched successfully (PID: {}): {}", pid, tool.name));
    Ok(format!("Launched: {} (PID: {})", tool.name, pid))
}

fn execute_http_tool(tool: &ToolConfig, _params: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    let http_config = tool.http.as_ref()
        .ok_or("HTTP backend requires http config")?;

    log(&format!("Executing HTTP tool: {} {} {}", http_config.method, http_config.url, http_config.body));

    let client = reqwest::blocking::Client::new();

    let response = match http_config.method.to_uppercase().as_str() {
        "POST" => {
            client.post(&http_config.url)
                .header("Content-Type", "application/json")
                .body(http_config.body.clone())
                .send()?
        }
        "GET" => {
            client.get(&http_config.url).send()?
        }
        method => return Err(format!("Unsupported HTTP method: {}", method).into()),
    };

    if response.status().is_success() {
        log(&format!("HTTP tool executed successfully: {}", tool.name));
        Ok(format!("HTTP request succeeded: {}", response.status()))
    } else {
        log(&format!("HTTP tool failed: {}", response.status()));
        Err(format!("HTTP request failed: {}", response.status()).into())
    }
}

/// Substitute {param} placeholders in strings
fn substitute_params(template: &str, params: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    let mut result = template.to_string();

    // Find all {param} patterns
    let re = regex::Regex::new(r"\{(\w+)\}")?;

    for cap in re.captures_iter(template) {
        let param_name = &cap[1];
        let placeholder = &cap[0];

        let value = params.get(param_name)
            .ok_or(format!("Missing parameter: {}", param_name))?;

        let value_str = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => return Err(format!("Invalid parameter type for {}", param_name).into()),
        };

        result = result.replace(placeholder, &value_str);
    }

    Ok(result)
}

fn get_config_path() -> Result<PathBuf, Box<dyn Error>> {
    let home = std::env::var("HOME")?;
    Ok(PathBuf::from(home).join(".config/transcribe-rs/tools.json"))
}

fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/ptt_rust_debug.log")
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let _ = writeln!(file, "[{}] [tools] {}", timestamp, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_params() {
        let params = serde_json::json!({
            "workspace": 3,
            "task": "hello world"
        });

        let result = substitute_params("workspace {workspace}", &params).unwrap();
        assert_eq!(result, "workspace 3");

        let result = substitute_params("{task}", &params).unwrap();
        assert_eq!(result, "hello world");
    }
}
