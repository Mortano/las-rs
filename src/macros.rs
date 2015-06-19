//! Internal-use macros.

/// Reads n bytes into a buffer, or returns a `ReadError`.
macro_rules! try_read_n {
    ($reader:expr, $data:expr, $n:expr) => {{
        let took = try!($reader.take($n).read(&mut $data));
        if took != $n {
            return Err(Error::ReadError(format!("Tried to take {} bytes, only took {}", $n, took)));
        }
    }};
}
