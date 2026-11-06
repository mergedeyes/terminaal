//! What differs between Unix and Windows underneath the features: the
//! connected local stream pairs the workers use to wake each other and
//! to tunnel bytes ([`pair`]), and waiting on several sockets at once
//! ([`poll`]). On Unix these are exactly the socketpairs and `poll(2)`
//! the code always used; on Windows a loopback TCP pair and `WSAPoll`.

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_pair_carries_bytes_both_ways() {
        let (mut a, mut b) = pair().unwrap();
        a.write_all(b"ping").unwrap();
        let mut buf = [0; 4];
        b.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ping");
        b.write_all(b"pong").unwrap();
        a.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"pong");
    }

    #[test]
    fn poll_reports_which_handle_is_ready() {
        let (a, mut b) = pair().unwrap();
        let (c, _d) = pair().unwrap();
        let fds = [(a.handle(), POLLIN), (c.handle(), POLLIN)];
        assert_eq!(poll(&fds, Duration::from_millis(10)), [0, 0]);
        b.write_all(b"x").unwrap();
        let revents = poll(&fds, Duration::from_millis(1000));
        assert!(revents[0] & POLLIN != 0 && revents[1] == 0);
    }
}
