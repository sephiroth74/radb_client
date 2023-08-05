/// cargo test --color=always --bin randroid tests -- --test-threads=1 --show-output
#[cfg(test)]
mod tests {
    use std::fmt::{Display, Formatter};
    use std::fs::File;
    use std::io::{BufRead, Write};

    use std::os::fd::{AsRawFd, FromRawFd};
    use std::path::PathBuf;
    use std::process::Stdio;
    use std::str::FromStr;
    use std::sync::Once;
    use std::thread::sleep;
    use std::time::Duration;
    use std::{env, fs};

    use chrono::Local;
    use env_logger::fmt::Color;
    use futures::{StreamExt, TryFutureExt};
    use log::*;
    use once_cell::sync::Lazy;
    use regex::Regex;
    use tokio::process::Command;
    use tokio::sync::oneshot::{channel, Receiver, Sender};
    use tokio_util::codec::{FramedRead, LinesCodec};

    use crate::client::LogcatOptions;
    use crate::command::CommandBuilder;
    use crate::debug::CommandDebug;
    use crate::shell::{DumpsysPriority, ScreenRecordOptions, SettingsType};
    use crate::util::Vec8ToString;
    use crate::{Adb, AdbDevice, Client, Shell};

    static INIT: Once = Once::new();

    static ADB: Lazy<Adb> = Lazy::new(|| Adb::new().unwrap());

    static DEVICE_IP: Lazy<String> = Lazy::new(|| String::from("192.168.1.27"));

    static DEVICE: Lazy<Box<dyn AdbDevice>> = Lazy::new(|| ADB.device(DEVICE_IP.as_str()).unwrap());

    macro_rules! assert_connected {
        ($device:expr) => {
            let o = Client::connect(&ADB, $device.as_ref()).await;
            trace!("output = {:?}", o);
            debug_assert!(o.is_ok(), "device not connected");
            trace!("connected!");
        };
    }

    fn initialize() {
        INIT.call_once(|| {
            env_logger::builder()
                .default_format()
                .format(|buf, record| {
                    let mut buf_style = buf.style();
                    let default_styled_level = buf.default_level_style(record.level());

                    buf_style
                        .set_color(Color::Ansi256(8))
                        .set_dimmed(true)
                        .set_intense(false);

                    writeln!(
                        buf,
                        "{}{} {:>5}{} - {}",
                        buf_style.value("["),
                        default_styled_level.value(Local::now().format("%H:%M:%S:%3f")),
                        buf.default_styled_level(record.level()),
                        buf_style.value("]"),
                        default_styled_level.value(record.args())
                    )
                })
                .init();
        });
    }

    #[tokio::test]
    async fn test_connect() {
        initialize();
        assert_connected!(&DEVICE);
    }

    #[tokio::test]
    async fn test_is_connected() {
        initialize();
        assert_connected!(&DEVICE);
        assert!(Client::is_connected(&ADB, DEVICE.as_ref()).await);
    }

    #[tokio::test]
    async fn test_whoami() {
        initialize();
        assert_connected!(&DEVICE);
        let output = Shell::whoami(&ADB, DEVICE.as_ref())
            .await
            .expect("whoami failed");
        debug_assert!(output.is_some(), "unknown whoami");
    }

    #[tokio::test]
    async fn test_remount() {
        initialize();
        assert_connected!(&DEVICE);
        Client::root(&ADB, DEVICE.as_ref())
            .await
            .expect("root failed");
        Client::remount(&ADB, DEVICE.as_ref())
            .await
            .expect("remount failed");
    }

    #[tokio::test]
    async fn test_disable_verity() {
        initialize();
        assert_connected!(&DEVICE);
        Client::root(&ADB, DEVICE.as_ref())
            .await
            .expect("root failed");
        Client::disable_verity(&ADB, DEVICE.as_ref())
            .await
            .expect("disable-verity failed");
    }

    #[tokio::test]
    async fn test_root() {
        initialize();
        assert_connected!(&DEVICE);
        let success = Client::root(&ADB, DEVICE.as_ref())
            .await
            .expect("Unable to root device");
        debug_assert!(success, "root failed");
        sleep(Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_is_root() {
        initialize();
        assert_connected!(&DEVICE);
        Shell::is_root(&ADB, DEVICE.as_ref())
            .await
            .expect("is_root failed");
    }

    #[tokio::test]
    async fn test_which() {
        initialize();
        assert_connected!(&DEVICE);
        let w = Shell::which(&ADB, DEVICE.as_ref(), "busybox")
            .await
            .expect("which failed");
        debug_assert!(w.is_some(), "which failed");
        let result = w.unwrap();
        trace!("result: {:?}", result);
        assert_eq!(result.as_str(), "/vendor/bin/busybox");
    }

    #[tokio::test]
    async fn test_getprop() {
        initialize();
        assert_connected!(&DEVICE);
        let output = Shell::getprop(&ADB, DEVICE.as_ref(), "wifi.interface")
            .await
            .expect("getprop failed");
        assert_eq!("wlan0", output.as_str().unwrap().trim_end());
    }

    #[tokio::test]
    async fn test_cat() {
        initialize();
        assert_connected!(&DEVICE);
        let output = Shell::cat(&ADB, DEVICE.as_ref(), "/timeshift/conf/tvlib-aot.properties")
            .await
            .expect("cat failed");
        assert!(output.lines().into_iter().all(|f| f.is_ok()));
        assert!(output
            .lines()
            .into_iter()
            .filter(|f| f.is_ok())
            .all(|l| l.is_ok()));
    }

    #[tokio::test]
    async fn test_getprops() {
        initialize();
        assert_connected!(&DEVICE);
        let properties = Shell::getprops(&ADB, DEVICE.as_ref())
            .await
            .expect("getprops failed");
        assert!(properties.len() > 0);
    }

    #[tokio::test]
    async fn test_exists() {
        initialize();
        assert_connected!(&DEVICE);
        let exists = Shell::exists(&ADB, DEVICE.as_ref(), "/timeshift/conf/tvlib-aot.properties")
            .await
            .unwrap();
        assert_eq!(true, exists);
    }

    #[tokio::test]
    async fn test_is_file() {
        initialize();
        assert_connected!(&DEVICE);
        let f1 = Shell::is_file(&ADB, DEVICE.as_ref(), "/timeshift/conf/tvlib-aot.properties")
            .await
            .unwrap();
        assert_eq!(true, f1);

        let f2 = Shell::is_file(&ADB, DEVICE.as_ref(), "/timeshift/conf/")
            .await
            .unwrap();
        assert_eq!(false, f2);
    }

    #[tokio::test]
    async fn test_is_dir() {
        initialize();
        assert_connected!(&DEVICE);
        let f1 = Shell::is_dir(&ADB, DEVICE.as_ref(), "/timeshift/conf/tvlib-aot.properties")
            .await
            .unwrap();
        assert_eq!(false, f1);
        let f2 = Shell::is_dir(&ADB, DEVICE.as_ref(), "/timeshift/conf/")
            .await
            .unwrap();
        assert_eq!(true, f2);
    }

    #[tokio::test]
    async fn test_disconnect() {
        initialize();
        assert_connected!(&DEVICE);
        assert!(Client::disconnect(&ADB, DEVICE.as_ref())
            .await
            .expect("disconnect failed"));
    }

    #[tokio::test]
    async fn test_disconnect_all() {
        initialize();
        assert!(Client::disconnect_all(&ADB)
            .await
            .expect("disconnect all failed"));
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    struct DeviceFile {
        raw_value: String,
        perms: String,
        links: i128,
        owner: String,
        group: String,
        size: i64,
        date: String,
        time: String,
        name: String,
    }

    #[derive(Debug, Clone)]
    struct ParseError;

    impl Display for ParseError {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            write!(f, "failed to parse line")
        }
    }

    impl Display for DeviceFile {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "{:} {:} {:} {:} {:} {:} {:} {:}",
                self.perms,
                self.links,
                self.owner,
                self.group,
                self.size,
                self.date,
                self.time,
                self.file_name()
            )
        }
    }

    impl FromStr for DeviceFile {
        type Err = ParseError;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            let re = Regex::new("\\s+").unwrap();
            let fields: Vec<&str> = re.splitn(s, 8).collect();

            if fields.len() < 8 {
                return Err(ParseError);
            }

            let perms = fields.get(0).unwrap().to_string();
            let links = fields
                .get(1)
                .unwrap()
                .parse::<i128>()
                .map_err(|_| ParseError)?;
            let owner = fields.get(2).unwrap().to_string();
            let group = fields.get(3).unwrap().to_string();
            let size = fields
                .get(4)
                .unwrap()
                .parse::<i64>()
                .map_err(|_| ParseError)?;
            let date = fields.get(5).unwrap().to_string();
            let time = fields.get(6).unwrap().to_string();
            let name = fields.get(7).unwrap().to_string();

            Ok(DeviceFile {
                raw_value: s.to_string(),
                perms,
                links,
                owner,
                group,
                size,
                date,
                time,
                name,
            })
        }
    }

    impl DeviceFile {
        #[inline]
        pub fn parse<F: FromStr>(s: &str) -> Result<F, F::Err> {
            FromStr::from_str(s)
        }

        pub fn file_name(&self) -> &str {
            if self.is_symlink() {
                self.get_src_symlink_name()
            } else {
                self.name.as_str()
            }
        }

        fn get_src_symlink_name(&self) -> &str {
            let v: Vec<&str> = self.name.splitn(2, " -> ").collect();
            v.get(0).unwrap().trim()
        }

        #[allow(dead_code)]
        fn get_dst_symlink_name(&self) -> &str {
            let v: Vec<&str> = self.name.splitn(2, " -> ").collect();
            v.get(1).unwrap().trim()
        }

        pub fn is_dir(&self) -> bool {
            self.test_perm(b'd')
        }

        pub fn is_file(&self) -> bool {
            self.test_perm(b'-')
        }

        pub fn is_symlink(&self) -> bool {
            self.test_perm(b'l')
        }

        fn test_perm(&self, c: u8) -> bool {
            self.perms.as_bytes()[0] == c
        }
    }

    #[tokio::test]
    async fn test_list_dir() {
        initialize();
        assert_connected!(&DEVICE);
        let lines = Shell::list_dir(&ADB, DEVICE.as_ref(), "/system")
            .await
            .expect("list dir failed");
        for line in lines {
            let file: Result<DeviceFile, ParseError> = DeviceFile::parse(line.as_str());
            if file.is_ok() {
                let f = file.unwrap();
                trace!("{:}", f);

                if f.file_name() == "vendor" {
                    assert!(f.is_symlink());
                    assert!(!f.is_dir());
                    assert!(!f.is_file());
                }
            }
        }
    }

    #[tokio::test]
    async fn test_list_settings() {
        initialize();
        assert_connected!(&DEVICE);
        let settings = Shell::list_settings(&ADB, DEVICE.as_ref(), SettingsType::system)
            .await
            .expect("list settings failed");
        assert!(settings.len() > 0);
        eprintln!("{:#?}", settings);

        for s in settings {
            let value = Shell::get_setting(&ADB, DEVICE.as_ref(), SettingsType::system, s.key.as_str())
                .await
                .expect("get setting failed")
                .expect("parse value failed");
            eprintln!("{} = {} [{:}]", s.key, s.value, value);
        }
    }

    #[tokio::test]
    async fn test_list_dumpsys() {
        initialize();
        assert_connected!(&DEVICE);
        let output = Shell::dumpsys_list(&ADB, DEVICE.as_ref(), false, Some(DumpsysPriority::CRITICAL))
            .await
            .expect("dumpsys failed");

        for line in output {
            trace!("{:?}", line);
        }
    }

    #[tokio::test]
    async fn test_screen_mirror() {
        initialize();
        assert_connected!(&DEVICE);
        Client::root(&ADB, DEVICE.as_ref()).await.unwrap();

        tokio::join!(async {
            let child1 = Command::new("adb")
                //.args(vec!["-s", "192.168.1.111:5555", "shell", "ls", "-la /timeshift/conf"])
                .args(vec![
                    "-s",
                    "192.168.1.111:5555",
                    "shell",
                    "while true; do screenrecord --output-format=h264 -; done",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .debug()
                .spawn()
                .unwrap();

            let mut command2 = Command::new("ffplay");

            command2
                .args(vec![
                    "-framerate",
                    "60",
                    "-probesize",
                    "32",
                    "-sync",
                    "video",
                    "-",
                ])
                .stdout(Stdio::piped());

            unsafe {
                let fd = child1.stdout.as_ref().unwrap().as_raw_fd();
                command2.stdin(Stdio::from_raw_fd(fd));
            }

            //command2.stdin(Stdio::from(child1.stdout.unwrap()))
            let child2 = command2.debug().spawn().unwrap();

            //let output = child2.wait_with_output().unwrap();
            let output = child2.wait_with_output().await.unwrap();

            trace!("exit status: {:?}", output.status);

            if output.status.success() {
                for line in output.stdout.lines() {
                    debug!("stdout => {:}", line.unwrap().trim_end());
                }
            } else {
                for line in output.stderr.lines() {
                    warn!("stderr => {:}", line.unwrap().trim_end());
                }
            }
        });
    }

    #[tokio::test]
    async fn test_command_pipe() {
        initialize();

        tokio::join!(async {
            let mut cmd1 = CommandBuilder::shell(&ADB, DEVICE.as_ref());
            cmd1.arg("while true; do screenrecord --output-format=h264 -; done");

            let mut cmd2 = CommandBuilder::new("ffplay");
            cmd2.args(vec![
                "-framerate",
                "60",
                "-probesize",
                "32",
                "-sync",
                "video",
                "-",
            ]);

            let output = CommandBuilder::pipe(&mut cmd1, &mut cmd2)
                .await
                .unwrap()
                .wait_with_output()
                .await
                .unwrap();

            trace!("exit status: {:?}", output.status);

            if output.status.success() {
                for line in output.stdout.lines() {
                    debug!("stdout => {:}", line.unwrap().trim_end());
                }
            } else {
                for line in output.stderr.lines() {
                    warn!("stderr => {:}", line.unwrap().trim_end());
                }
            }
        });
    }

    #[tokio::test]
    async fn test_save_screencap() {
        initialize();
        assert_connected!(&DEVICE);

        Shell::exists(&ADB, DEVICE.as_ref(), "/sdcard/Download")
            .await
            .unwrap();
        Shell::save_screencap(&ADB, DEVICE.as_ref(), "/sdcard/Download/screencap.png")
            .await
            .expect("save screencap failed");
    }

    #[tokio::test]
    async fn test_is_screen_on() {
        initialize();
        assert_connected!(&DEVICE);

        eprintln!("connected!");

        let result = Shell::is_screen_on(&ADB, DEVICE.as_ref())
            .await
            .expect("is screen on failed");
        assert_eq!(result, true);
    }

    #[tokio::test]
    async fn test_screen_record() {
        initialize();
        assert_connected!(&DEVICE);

        let mut options = ScreenRecordOptions::default();
        options.verbose = true;
        options.timelimit = Some(Duration::from_secs(12));

        let remote_file = String::from("/sdcard/Download/screenrecord.mp4");
        let local_file = env::current_dir().unwrap().join("screenrecord.mp4");

        if local_file.exists() {
            fs::remove_file(&local_file).unwrap();
        }

        let (send, recv): (Sender<()>, Receiver<()>) = channel::<()>();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.unwrap();
            trace!("ctrl+c pressed!");
            send.send(())
        });

        match Shell::screen_record(&ADB, DEVICE.as_ref(), Some(options), remote_file.as_str(), Some(recv.into_future())).await {
            Ok(t) => trace!("Screen Record Ok: {:?}", t),
            Err(e) => {
                error!("{:}", e)
            }
        }

        trace!("need to sleep a bit..");
        sleep(Duration::from_secs(2));

        match Client::pull(&ADB, DEVICE.as_ref(), remote_file.as_str(), local_file.as_path()).await {
            Ok(t) => {
                debug!("Pull Ok: {:?}", t)
            }
            Err(e) => {
                error!("{:}", e)
            }
        }
    }

    #[tokio::test]
    async fn test_get_events() {
        initialize();
        assert_connected!(&DEVICE);
        let events = Shell::get_events(&ADB, DEVICE.as_ref()).await.unwrap();
        for event in events {
            trace!("event: {}, {}", event.0, event.1)
        }
    }

    #[tokio::test]
    async fn test_send_events() {
        initialize();
        assert_connected!(&DEVICE);
        // KEYCODE_DPAD_RIGHT (action DOWN)
        Shell::send_event(&ADB, DEVICE.as_ref(), "/dev/input/event3", 0x0001, 0x006a, 0x00000001)
            .await
            .unwrap();
        Shell::send_event(&ADB, DEVICE.as_ref(), "/dev/input/event3", 0x0000, 0x0000, 0x00000000)
            .await
            .unwrap();
        // KEYCODE_DPAD_RIGHT (action UP)
        Shell::send_event(&ADB, DEVICE.as_ref(), "/dev/input/event3", 0x0001, 0x006a, 0x00000000)
            .await
            .unwrap();
        Shell::send_event(&ADB, DEVICE.as_ref(), "/dev/input/event3", 0x0000, 0x0000, 0x00000000)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_command() {
        initialize();

        let output_file = File::create("output.txt").unwrap();
        let mut builder = CommandBuilder::shell(&ADB, DEVICE.as_ref());

        builder
            .args(vec!["ls", "-la", "/"])
            .stdout(Stdio::from(output_file));
        trace!("builder: {:?}", builder);

        let output = builder.output().await.unwrap();
        debug!("output: {:?}", output);
    }

    #[tokio::test]
    async fn test_clear_logcat() {
        initialize();
        assert_connected!(&DEVICE);
        Client::clear_logcat(&ADB, DEVICE.as_ref()).await.unwrap();
    }

    #[tokio::test]
    async fn test_list_devices() {
        initialize();
        let adb = Adb::new().expect("adb failed");
        let devices = adb.devices().await.expect("failed to list devices");
        debug!("Found {} devices", devices.len());

        for device in devices {
            eprintln!("Found device {:#?}", device);
        }
    }

    #[tokio::test]
    async fn test_push() {
        initialize();
        assert_connected!(&DEVICE);

        let remote_path = PathBuf::from("/sdcard/Download/text.txt");

        let mut local_path = env::current_dir().unwrap();
        local_path.push("test.txt");

        let mut file = File::create(&local_path).unwrap();
        file.write("hello world".as_bytes()).unwrap();
        file.flush().unwrap();

        if Shell::exists(&ADB, DEVICE.as_ref(), remote_path.as_path().to_str().unwrap())
            .await
            .unwrap()
        {
            Shell::exec(&ADB, DEVICE.as_ref(), vec!["rm", remote_path.as_path().to_str().unwrap()], None)
                .await
                .unwrap();
        }

        let result = Client::push(&ADB, DEVICE.as_ref(), local_path.as_path(), remote_path.as_path().to_str().unwrap())
            .await
            .unwrap();
        trace!("{}", result);

        assert!(Shell::exists(&ADB, DEVICE.as_ref(), remote_path.as_path().to_str().unwrap())
            .await
            .unwrap());
        Shell::exec(&ADB, DEVICE.as_ref(), vec!["rm", remote_path.as_path().to_str().unwrap()], None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_logcat() {
        initialize();
        assert_connected!(&DEVICE);

        let (send, recv): (Sender<()>, Receiver<()>) = channel::<()>();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.unwrap();
            trace!("Ctrl+c pressed!");
            send.send(())
        });

        let timeout = Some(Duration::from_secs(5));
        let since = Some(Local::now() - chrono::Duration::seconds(30));

        let options = LogcatOptions {
            expr: None,
            dump: false,
            filename: None,
            tags: None,
            format: None,
            since: since,
            pid: None,
            timeout: timeout,
        };

        let output = Client::logcat(&ADB, DEVICE.as_ref(), options, Some(recv.into_future())).await;

        match output {
            Ok(o) => {
                if o.is_success() || o.is_kill() || o.is_interrupt() {
                    let stdout = o.stdout();
                    let lines = stdout.lines().map(|l| l.unwrap());
                    for line in lines {
                        trace!("{}", line);
                    }
                } else if o.has_stderr() {
                    warn!("{}", o);
                } else {
                    error!("{}", o);
                }
            }
            Err(err) => {
                warn!("{}", err);
            }
        }
    }

    #[tokio::test]
    async fn test_client_api_level() {
        initialize();
        assert_connected!(&DEVICE);
        let api_level = Client::api_level(&ADB, DEVICE.as_ref()).await.unwrap();
        assert!(api_level > 0);
        trace!("api level: {:?}", api_level);
    }

    #[tokio::test]
    async fn test_client_name() {
        initialize();
        assert_connected!(&DEVICE);
        let name = Client::name(&ADB, DEVICE.as_ref()).await.unwrap();
        assert!(name.is_some());

        let string = name.unwrap();
        assert!(string.len() > 0);

        trace!("name: {:?}", string);
    }

    #[tokio::test]
    async fn test_client_version() {
        initialize();
        assert_connected!(&DEVICE);
        let name = Client::version(&ADB, DEVICE.as_ref()).await.unwrap();
        trace!("client version: {:?}", name);
    }

    #[tokio::test]
    async fn test_stream() {
        initialize();
        assert_connected!(&DEVICE);

        trace!("ok, connected...");

        let mut cmd = CommandBuilder::new("adb");
        cmd.arg("logcat");

        cmd.stdout(Stdio::piped());
        trace!("Now spawning the child...");

        //let output = cmd.output();
        let mut child = cmd.spawn().await.expect("failed to spawn command");

        let stdout = child
            .stdout
            .take()
            .expect("child did not have a handle to stdout");
        let mut reader = FramedRead::new(stdout, LinesCodec::new());
        let (tx, rx) = channel::<()>();

        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.unwrap();
            tx.send(())
        });

        tokio::spawn(async move {
            trace!("spawned...");

            let sleep = tokio::time::sleep(Duration::from_secs(1));
            tokio::select! {
                _ = rx => {
                    warn!("CTRL+C received!");
                    child.kill().await.unwrap();
                },
                _ = child.wait() => {
                    warn!("Child exited normally")
                },
                _ = (sleep) => {
                    warn!("Timeout expired!");
                    child.kill().await.unwrap();
                },
            }
        });

        while let Some(line) = reader.next().await {
            trace!("Line: {}", line.unwrap());
        }

        debug!("Ok. done");
    }
}
