use std::fs::OpenOptions;
use std::io::Write;
use std::thread;
use std::time::Duration;
use chrono::Local;

fn main() {
    let log_file = "/tmp/running.INFO";

    println!("开始写日志到 {}", log_file);

    let mut counter = 0;
    loop {
        counter += 1;

        // 打开文件，追加模式
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)
            .expect("无法打开日志文件");

        // 写入日志
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let log_line = format!("[{}] [INFO] 这是第 {} 条日志\n", timestamp, counter);

        file.write_all(log_line.as_bytes())
            .expect("写入日志失败");

        println!("写入: {}", log_line.trim());

        // 每 500ms 写一条
        thread::sleep(Duration::from_millis(500));
    }
}
