use std::cell::RefCell;

#[derive(Clone, Debug)]
pub struct Frame {
    pub file: String,
    pub fn_name: String,
    pub line: u32,
    pub col: u32,
}

thread_local! {
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

pub fn push(file: String, fn_name: String, line: u32, col: u32) {
    FRAMES.with(|f| f.borrow_mut().push(Frame { file, fn_name, line, col }));
}

pub fn pop() {
    FRAMES.with(|f| { f.borrow_mut().pop(); });
}

pub fn depth() -> usize {
    FRAMES.with(|f| f.borrow().len())
}

/// Formats the current frame stack like Bun/Node: "      at fn (file:line:col)\n"
pub fn format_stack() -> String {
    FRAMES.with(|f| {
        let frames = f.borrow();
        if frames.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for frame in frames.iter().rev() {
            if frame.line > 0 {
                out.push_str(&format!(
                    "      at {} ({}:{}:{})\n",
                    frame.fn_name, frame.file, frame.line, frame.col
                ));
            } else {
                out.push_str(&format!("      at {} ({})\n", frame.fn_name, frame.file));
            }
        }
        out
    })
}

pub fn capture_string() -> String {
    format_stack()
}
