use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
};

pub struct LogRing {
    max: usize,
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    lines: VecDeque<String>,
    partial: Vec<u8>,
}
pub type LogSource = Arc<LogRing>;

impl LogRing {
    pub fn new(max: usize) -> Self {
        Self {
            max,
            state: Mutex::new(State::default()),
        }
    }
    pub fn write(&self, b: &[u8]) {
        let mut state = self.state.lock().unwrap();
        for part in b.split_inclusive(|c| *c == b'\n') {
            state.partial.extend_from_slice(part);
            if part.last() == Some(&b'\n') {
                state.partial.pop();
                let line = String::from_utf8_lossy(&state.partial).into_owned();
                state.lines.push_back(line);
                state.partial.clear();
                while state.lines.len() > self.max {
                    state.lines.pop_front();
                }
            }
        }
    }
    pub fn tail(&self, n: usize) -> Vec<String> {
        let state = self.state.lock().unwrap();
        state
            .lines
            .iter()
            .skip(state.lines.len().saturating_sub(n))
            .cloned()
            .collect()
    }
}
impl io::Write for &LogRing {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        LogRing::write(self, buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_ring() {
        let r = LogRing::new(3);
        r.write(b"a\nb\nc");
        r.write(b"d\ne\nf");
        assert_eq!(r.tail(10), ["b", "cd", "e"]);
        assert_eq!(r.tail(1), ["e"]);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..100 {
                        r.write(b"x\n");
                    }
                });
            }
        });
        assert_eq!(r.tail(10), ["x", "x", "x"]);
    }
    #[test]
    fn utf8_split_across_writes() {
        let r = LogRing::new(1);
        r.write(&[0xc3]);
        r.write(&[0xa9, b'\n']);
        assert_eq!(r.tail(1), ["é"]);
    }
}
