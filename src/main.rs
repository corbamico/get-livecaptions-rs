use chrono::prelude::*;
use std::process;
use tokio::time::Duration;

use clap::{Parser, builder::PossibleValuesParser};
use log::{error, info};
use windows::{
    Win32::{
        System::{Com::*, Variant::VARIANT},
        UI::{Accessibility::*, WindowsAndMessaging::*},
    },
    core::*,
};

use anyhow::{Result, anyhow};
use libretranslate::{Language, translate_url};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the file to output
    #[arg(short, long)]
    file: String,

    /// Enable translation from source language to English
    #[arg(long, value_parser = PossibleValuesParser::new(["ar","zh","fr","de","it","ja","pt","ru","es","pl"]))]
    translate: Option<String>,

    /// LibreTranslate server host
    #[arg(long, default_value = "http://127.0.0.1:5000", requires = "translate")]
    // Default to localhost
    translate_host: String,
}

struct Engine {
    automation: IUIAutomation,
    condition: IUIAutomationCondition,
    prebuffer: String,
    translate_buffer: String, // NEW: buffer for checking translation-only (not disk)
    sfilename: String,
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
impl Engine {
    fn new(sfilename: &str) -> Self {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .expect("Failed initial Windows COM.");
        };

        let automation: IUIAutomation = unsafe {
            CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)
                .expect("Failed initial Windows Accessibility API.")
        };
        let condition = unsafe {
            automation
                .CreatePropertyCondition(
                    UIA_AutomationIdPropertyId,
                    &VARIANT::from("CaptionsTextBlock"),
                )
                .unwrap()
        };

        Self {
            automation,
            condition,
            prebuffer: Default::default(),
            translate_buffer: Default::default(),
            sfilename: sfilename.to_string(),
        }
    }
    fn get_livecaptions(&self) -> Result<String> {
        let window = unsafe { FindWindowW(w!("LiveCaptionsDesktopWindow"), None) }?;
        let element = unsafe { self.automation.ElementFromHandle(window) }?;
        let text = unsafe { element.FindFirst(TreeScope_Descendants, &self.condition) }?;
        let text = unsafe { text.CurrentName() }?;
        Ok(text.to_string())
    }

    async fn translate_new_content(&mut self, from: &str, host: &str) -> Result<()> {
        if let Ok(current_text) = self.get_livecaptions() {
            // Extract only the new content by comparing with previous state
            let new_content = extract_new_lines(&self.translate_buffer, &current_text);

            if !new_content.is_empty() && !new_content.trim().is_empty() {
                let translated = self.translate_text(&new_content, from, host).await?;
                eprintln!("\x1b[32m[en]\x1b[0m{}", translated);
            }
            // Update translate_buffer to current full text for next comparison
            self.translate_buffer = current_text;
        }
        Ok(())
    }

    fn save_current_captions(&mut self) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::prelude::*;

        if let Ok(current_text) = self.get_livecaptions() {
            // Extract only the new content by comparing with previous state
            let new_content = extract_new_lines(&self.prebuffer, &current_text);

            if !new_content.is_empty() && !new_content.trim().is_empty() {
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.sfilename)?;

                let local: DateTime<Local> = Local::now();
                writeln!(file, "{}", local.format("[%Y-%m-%d][%H:%M:%S]"))?;
                file.write_all(new_content.as_bytes())?;
                file.write_all(b"\n")?;
                file.sync_all()?;
            }
            // Update buffer to current full text for next comparison
            self.prebuffer = current_text;
        }
        Ok(())
    }

    fn graceful_shutdown(&mut self) -> Result<()> {
        self.save_current_captions()?;
        Ok(())
    }
    async fn translate_text(&self, text: &str, from: &str, host: &str) -> Result<String> {
        let source = match from {
            "ar" => Language::Arabic,
            "zh" => Language::Chinese,
            "fr" => Language::French,
            "de" => Language::German,
            "it" => Language::Italian,
            "ja" => Language::Japanese,
            "pt" => Language::Portuguese,
            "ru" => Language::Russian,
            "es" => Language::Spanish,
            "pl" => Language::Polish,
            _ => return Err(anyhow!("Unsupported language code: {}", from)),
        };

        let target = Language::English;

        let data = translate_url(source, target, text, host, None)
            .await
            .map_err(|e| anyhow!("Translation failed: {:?}", e))?;

        Ok(data.output)
    }
}

// Standalone function for extracting new lines (easier to test)
fn extract_new_lines(previous: &str, current: &str) -> String {
    // First run - all content is new
    if previous.is_empty() {
        return current.to_string();
    }

    let prev_lines: Vec<&str> = previous.lines().collect();
    let curr_lines: Vec<&str> = current.lines().collect();

    if prev_lines.is_empty() {
        return current.to_string();
    }

    // Try to find any suffix of previous (starting from any position) that matches
    // a prefix of current. This handles cases where old lines are truncated.
    let mut best_match_len = 0;

    // Scan through all possible starting positions in previous
    for start_idx in 0..prev_lines.len() {
        // Try to match consecutive lines from this position with the start of current
        let mut match_len = 0;
        let max_possible = (prev_lines.len() - start_idx).min(curr_lines.len());

        for i in 0..max_possible {
            if prev_lines[start_idx + i] == curr_lines[i] {
                match_len += 1;
            } else {
                break; // Stop at first mismatch
            }
        }

        if match_len > best_match_len {
            best_match_len = match_len;
        }
    }

    if best_match_len > 0 {
        // Found overlap - new content starts after the overlap
        if best_match_len < curr_lines.len() {
            let new_content = curr_lines[best_match_len..].join("\n");
            // Add trailing newline for consistency, unless it's a complete match
            if !new_content.is_empty() {
                return new_content + "\n";
            }
            return new_content;
        } else {
            // No new content
            return String::new();
        }
    }

    // No overlap found - likely complete truncation, treat all as new
    current.to_string()
}

fn is_livecaptions_running() -> bool {
    unsafe { FindWindowW(w!("LiveCaptionsDesktopWindow"), None).is_ok() }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();
    info!("get-livecaptions running.");

    if !is_livecaptions_running() {
        error!("Please start Live Captions first. Program exiting.");
        return;
    }
    let mut engine = Engine::new(&args.file);

    let mut windows_timer = tokio::time::interval(Duration::from_secs(10));
    let mut writefile_timer = tokio::time::interval(Duration::from_secs(60));

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    let enable_translate = args.translate.is_some();

    let translate_lang = if enable_translate {
        args.translate.clone().unwrap()
    } else {
        String::new()
    };

    let translate_host = if enable_translate {
        args.translate_host.clone()
    } else {
        String::new()
    };

    println!(
        "get-livecaptions is running now, and save content into '{}', every 60s. ctrl-c for exit.",
        args.file
    );
    loop {
        tokio::select! {
            _ = windows_timer.tick() => {
                log::info!("Running check every 10s.");
                if !is_livecaptions_running()
                {
                    println!("Live captions is not running. Program exiting.");
                    let _ = engine.graceful_shutdown();
                    process::exit(0);
                }

                if enable_translate
                    && let Err(err) = engine.translate_new_content(&translate_lang, &translate_host).await {
                        log::error!("Translation error: {:?}", err);
                    }
            },
            _ = writefile_timer.tick() => {
                log::info!("Saving content to file every 60s.");
                if let Err(err) = engine.save_current_captions() {
                    log::error!("Failed to save file: {}", err);
                }
            },
            _ = &mut ctrl_c => {
                let _ = engine.graceful_shutdown();
                process::exit(0);
            }
        };
    }
}

#[cfg(test)]
mod tests;
