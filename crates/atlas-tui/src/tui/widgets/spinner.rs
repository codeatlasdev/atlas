const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn frame(tick: usize) -> &'static str {
    FRAMES[tick % FRAMES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_frames_cycle() {
        assert_eq!(frame(0), "⠋");
        assert_eq!(frame(9), "⠏");
        assert_eq!(frame(10), "⠋"); // wraps
    }
}
