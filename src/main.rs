use tokio::time::Duration;
use std::process;
use chrono::prelude::*;

use windows::{
    core::*, Win32::{System::Com::*, UI::{Accessibility::*, WindowsAndMessaging::*}},
};
use clap::Parser;
use log::{error,info};

use anyhow::Result;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the file to output
    #[arg(short, long)]
    file: String,

    /// interval of minutes for one cycle
    #[arg(short, long, default_value_t = 3,value_parser=clap::value_parser!(u8).range(1..6))]
    interval: u8,
}

struct Engine
{
        automation: IUIAutomation,
        condition:  IUIAutomationCondition,
        prebuffer: String,
        sfilename:String,
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe{CoUninitialize();}
    }
}
impl Engine {    
    fn new(sfilename:&str)->Self{
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok().expect("Failed initial Winodws COM.");};        

        let automation:IUIAutomation =  unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL).expect("Failed initial Winodws Accessibility API.") };
        let condition  = unsafe { automation.CreatePropertyCondition(UIA_AutomationIdPropertyId,  &VARIANT::from("CaptionsTextBlock")).unwrap()};
        Self {automation,condition,
            prebuffer:Default::default(),
            sfilename:sfilename.to_string(),
        }
    }
    fn get_livecaptions(&self) -> Result<String> {
        let window = unsafe { FindWindowW(w!("LiveCaptionsDesktopWindow"), None) };
        let element = unsafe { self.automation.ElementFromHandle(window) }?;
        let text = unsafe { element.FindFirst(TreeScope_Descendants, &self.condition) }?;
        let text =unsafe { text.CurrentName()}?;
        Ok(text.to_string())
    }
    fn save_current_captions(&mut self,current:&str,include_last_line:bool)->Result<()> 
    {
        use std::fs::OpenOptions;
        use std::io::prelude::*;
        let last_line = if !include_last_line {1} else {0};

         //从current的所有行中，找到第一行不在prebuffer的行 x 
        //将 行 x 到 current 倒数第2行，加入到prebuffer之后
        //最后一行ms livecaption会修正，所以不实时写入，在graceful_shutdown中，再写入。
        let mut lines: Vec<&str> = current.lines().collect();
        let mut first_new_line = None;
    
        // 找到第一个不在 prebuffer 中的行
        for (i, line) in lines.iter().enumerate() {
            if !self.prebuffer.contains(line) {
                first_new_line = Some(i);
                break;
            }
        }
        if let Some(start) = first_new_line {
            // 将新行添加到 prebuffer 中
            let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.sfilename)?;
            
            let local: DateTime<Local> = Local::now();
            write!(file, "{}\n", local.format("[%Y-%m-%d][%H:%M:%S]"))?;
            for line in lines.drain(start..lines.len() - last_line) {
                self.prebuffer.push_str(line);
                self.prebuffer.push('\n');

                file.write_all(line.as_bytes())?;
                file.write(b"\n")?;

            }
        }                
        Ok(())
    }
    fn graceful_shutdown(&mut self)->Result<()> {
        let text = self.get_livecaptions()?;
        self.save_current_captions(&text,true)?;
        Ok(())
    }
}

fn is_livecaptions_running()->bool{   
    return unsafe{FindWindowW(w!("LiveCaptionsDesktopWindow"), None).0}!=0;
}


#[tokio::main]
async fn main(){

    env_logger::init();
    let args = Args::parse();
    info!("get-livecaptions running.");

    if !is_livecaptions_running()
    {
        error!("livecaptions is not running. programe exiting.");
        return;
    }
    let mut engine = Engine::new(&args.file);

    let mut windows_timer = tokio::time::interval(Duration::from_secs(10));
    let mut writefile_timer = tokio::time::interval(Duration::from_secs(args.interval as u64 * 60));


    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    println!("get-livecaptions is running now, and save content into '{}', every {} min. ctrl-c for exit.",args.file, args.interval);
    loop{
        tokio::select!{
            _ = windows_timer.tick() => {
                log::info!("running checking, every 10s.");
                if !is_livecaptions_running()
                {
                    println!("livecaptions is not running. programe exiting.");
                    let _ = engine.graceful_shutdown();
                    process::exit(0);
                }
            },
            _ = writefile_timer.tick() => {
                log::info!("save content into file, every {} min.",args.interval);
                let text = engine.get_livecaptions();
                if let Ok(text) = text {
                    engine.save_current_captions(&text,false).expect("save file failed.");
                }                                            
            },
            _ = &mut ctrl_c => {
                let _ = engine.graceful_shutdown();
                process::exit(0);
            }
        };
    };
}

