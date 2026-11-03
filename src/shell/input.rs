//! `input`: key events, taps, swipes, text...

use std::time::Duration;

use strum::IntoStaticStr;

use crate::error::Result;
use crate::shell::{KeyCode, Shell};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, IntoStaticStr)]
#[allow(non_camel_case_types)]
/// The input source of the `input` command (`input <source> <command>`).
pub enum InputSource {
    /// A directional pad.
    dpad,
    /// A keyboard.
    keyboard,
    /// A mouse.
    mouse,
    /// A touchpad.
    touchpad,
    /// A gamepad.
    gamepad,
    /// A touch navigation device.
    touchnavigation,
    /// A joystick.
    joystick,
    /// A touchscreen.
    touchscreen,
    /// A stylus.
    stylus,
    /// A trackball.
    trackball,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, IntoStaticStr)]
#[allow(non_camel_case_types)]
/// The action of [`Shell::send_motion`] (`input motionevent <action>`).
pub enum MotionEvent {
    /// A pointer goes down.
    DOWN,
    /// A pointer goes up.
    UP,
    /// A pointer moves.
    MOVE,
    /// The gesture is cancelled.
    CANCEL,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
/// A special key event (`input keyevent --longpress | --doubletap`).
pub enum KeyEventType {
    /// `--longpress`
    LongPress,
    /// `--doubletap`
    DoubleTap,
}

impl KeyEventType {
    fn as_arg(&self) -> &'static str {
        match self {
            KeyEventType::LongPress => "--longpress",
            KeyEventType::DoubleTap => "--doubletap",
        }
    }
}

/// `input [source] <command>`
fn input(source: Option<InputSource>, command: &str) -> Vec<String> {
    let mut args = vec!["input".to_string()];
    if let Some(source) = source {
        let source: &str = source.into();
        args.push(source.to_string());
    }
    args.push(command.to_string());
    args
}

fn make_keyevent<I>(
    source: Option<InputSource>,
    event_type: Option<KeyEventType>,
    keys: I,
) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = input(source, "keyevent");
    if let Some(event_type) = event_type {
        args.push(event_type.as_arg().to_string());
    }
    args.extend(keys);
    args
}

fn make_keycombination<I>(source: Option<InputSource>, keys: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = input(source, "keycombination");
    args.extend(keys);
    args
}

fn make_tap(position: (i32, i32), source: Option<InputSource>) -> Vec<String> {
    let mut args = input(source, "tap");
    args.extend([position.0.to_string(), position.1.to_string()]);
    args
}

fn make_swipe(
    command: &str,
    from_pos: (i32, i32),
    to_pos: (i32, i32),
    duration: Option<Duration>,
    source: Option<InputSource>,
) -> Vec<String> {
    let mut args = input(source, command);
    args.extend([
        from_pos.0.to_string(),
        from_pos.1.to_string(),
        to_pos.0.to_string(),
        to_pos.1.to_string(),
    ]);
    if let Some(duration) = duration {
        args.push(duration.as_millis().to_string());
    }
    args
}

/// `input text` reads `%s` as a space.
fn make_text(text: &str, source: Option<InputSource>) -> Vec<String> {
    let mut args = input(source, "text");
    args.push(text.replace(' ', "%s"));
    args
}

fn make_motion(source: Option<InputSource>, motion: MotionEvent, pos: (i32, i32)) -> Vec<String> {
    let mut args = input(source, "motionevent");
    let motion: &str = motion.into();
    args.extend([motion.to_string(), pos.0.to_string(), pos.1.to_string()]);
    args
}

fn make_event(event: &str, code_type: i32, code: i32, value: i32) -> Vec<String> {
    vec![
        "sendevent".to_string(),
        event.to_string(),
        code_type.to_string(),
        code.to_string(),
        value.to_string(),
    ]
}

fn key_names<I, S>(keycodes: I) -> impl Iterator<Item = String>
where
    I: IntoIterator<Item = S>,
    S: Into<KeyCode>,
{
    keycodes.into_iter().map(|k| k.into().to_string())
}

fn key_numbers<I: IntoIterator<Item = u32>>(keycodes: I) -> impl Iterator<Item = String> {
    keycodes.into_iter().map(|k| k.to_string())
}

impl Shell<'_> {
    /// Send a key event (`input keyevent`).
    pub fn send_keyevent(
        &self,
        keycode: KeyCode,
        event_type: Option<KeyEventType>,
        source: Option<InputSource>,
    ) -> Result<()> {
        self.exec(make_keyevent(source, event_type, [keycode.to_string()]))
            .map(|_| ())
    }

    /// Send a key event by numeric key code.
    pub fn send_keycode(
        &self,
        keycode: u32,
        event_type: Option<KeyEventType>,
        source: Option<InputSource>,
    ) -> Result<()> {
        self.exec(make_keyevent(source, event_type, [keycode.to_string()]))
            .map(|_| ())
    }

    /// Send several key events, one after the other.
    pub fn send_keyevents<I, S>(&self, keycodes: I, source: Option<InputSource>) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: Into<KeyCode>,
    {
        self.exec(make_keyevent(source, None, key_names(keycodes)))
            .map(|_| ())
    }

    /// Send several key events by numeric key code, one after the other.
    pub fn send_keycodes<I>(&self, keycodes: I, source: Option<InputSource>) -> Result<()>
    where
        I: IntoIterator<Item = u32>,
    {
        self.exec(make_keyevent(source, None, key_numbers(keycodes)))
            .map(|_| ())
    }

    /// Press the keys together (e.g. `KEYCODE_CTRL_LEFT` + `KEYCODE_C`).
    pub fn send_keyevent_combination<I, S>(
        &self,
        source: Option<InputSource>,
        keycodes: I,
    ) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: Into<KeyCode>,
    {
        self.exec(make_keycombination(source, key_names(keycodes)))
            .map(|_| ())
    }

    /// Press the keys together, by numeric key code.
    pub fn send_keycode_combination<I>(
        &self,
        source: Option<InputSource>,
        keycodes: I,
    ) -> Result<()>
    where
        I: IntoIterator<Item = u32>,
    {
        self.exec(make_keycombination(source, key_numbers(keycodes)))
            .map(|_| ())
    }

    /// Tap the screen at `(x, y)` (`input tap`).
    pub fn send_tap(&self, position: (i32, i32), source: Option<InputSource>) -> Result<()> {
        self.exec(make_tap(position, source)).map(|_| ())
    }

    /// Swipe from `from_pos` to `to_pos` (`input swipe`), optionally in `duration`.
    pub fn send_swipe(
        &self,
        from_pos: (i32, i32),
        to_pos: (i32, i32),
        duration: Option<Duration>,
        source: Option<InputSource>,
    ) -> Result<()> {
        self.exec(make_swipe("swipe", from_pos, to_pos, duration, source))
            .map(|_| ())
    }

    /// Drag from `from_pos` and drop at `to_pos` (`input draganddrop`).
    pub fn send_draganddrop(
        &self,
        source: Option<InputSource>,
        duration: Option<Duration>,
        from_pos: (i32, i32),
        to_pos: (i32, i32),
    ) -> Result<()> {
        self.exec(make_swipe(
            "draganddrop",
            from_pos,
            to_pos,
            duration,
            source,
        ))
        .map(|_| ())
    }

    /// Press the trackball (`input press`).
    pub fn send_press(&self, source: Option<InputSource>) -> Result<()> {
        self.exec(input(source, "press")).map(|_| ())
    }

    /// Type a character (`input text`).
    pub fn send_char(&self, chr: char, source: Option<InputSource>) -> Result<()> {
        self.exec(make_text(&chr.to_string(), source)).map(|_| ())
    }

    /// Type a text. The text is sent as is: no shell expansion takes place.
    pub fn send_text(&self, text: &str, source: Option<InputSource>) -> Result<()> {
        self.exec(make_text(text, source)).map(|_| ())
    }

    /// Send a motion event at `pos` (`input motionevent`).
    pub fn send_motion(
        &self,
        source: Option<InputSource>,
        motion: MotionEvent,
        pos: (i32, i32),
    ) -> Result<()> {
        self.exec(make_motion(source, motion, pos)).map(|_| ())
    }

    /// Write a raw input event (`sendevent`). Root is usually required.
    pub fn send_event(&self, event: &str, code_type: i32, code: i32, value: i32) -> Result<()> {
        self.exec(make_event(event, code_type, code, value))
            .map(|_| ())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_keyevent() {
        assert_eq!(
            make_keyevent(None, None, [KeyCode::KEYCODE_HOME.to_string()]),
            ["input", "keyevent", "KEYCODE_HOME"]
        );
        assert_eq!(
            make_keyevent(
                Some(InputSource::dpad),
                Some(KeyEventType::LongPress),
                key_names([KeyCode::KEYCODE_1, KeyCode::KEYCODE_2])
            ),
            [
                "input",
                "dpad",
                "keyevent",
                "--longpress",
                "KEYCODE_1",
                "KEYCODE_2"
            ]
        );
        assert_eq!(
            make_keyevent(None, Some(KeyEventType::DoubleTap), key_numbers([3])),
            ["input", "keyevent", "--doubletap", "3"]
        );
    }

    #[test]
    fn test_keycombination() {
        assert_eq!(
            make_keycombination(
                Some(InputSource::keyboard),
                key_names([KeyCode::KEYCODE_CTRL_LEFT, KeyCode::KEYCODE_C])
            ),
            [
                "input",
                "keyboard",
                "keycombination",
                "KEYCODE_CTRL_LEFT",
                "KEYCODE_C"
            ]
        );
    }

    #[test]
    fn test_tap_swipe() {
        assert_eq!(make_tap((10, -20), None), ["input", "tap", "10", "-20"]);
        assert_eq!(
            make_swipe(
                "swipe",
                (1, 2),
                (3, 4),
                Some(Duration::from_millis(500)),
                Some(InputSource::touchscreen)
            ),
            ["input", "touchscreen", "swipe", "1", "2", "3", "4", "500"]
        );
        assert_eq!(
            make_swipe("draganddrop", (1, 2), (3, 4), None, None),
            ["input", "draganddrop", "1", "2", "3", "4"]
        );
    }

    #[test]
    fn test_text() {
        assert_eq!(
            make_text("hello world", None),
            ["input", "text", "hello%sworld"]
        );
        assert_eq!(
            make_text("it's $HOME", None),
            ["input", "text", "it's%s$HOME"]
        );
    }

    #[test]
    fn test_motion_and_event() {
        assert_eq!(
            make_motion(None, MotionEvent::DOWN, (5, 6)),
            ["input", "motionevent", "DOWN", "5", "6"]
        );
        assert_eq!(
            make_event("/dev/input/event0", 1, 116, 1),
            ["sendevent", "/dev/input/event0", "1", "116", "1"]
        );
    }
}
