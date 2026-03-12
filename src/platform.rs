#[cfg(unix)]
pub fn current_effective_uid() -> u32 {
    nix::unistd::geteuid().as_raw()
}

#[cfg(not(unix))]
pub fn current_effective_uid() -> u32 {
    0
}

#[cfg(unix)]
pub fn process_group_id(pid: u32) -> Option<u32> {
    use nix::unistd::{Pid, getpgid};

    getpgid(Some(Pid::from_raw(pid as i32)))
        .ok()
        .map(|pgid| pgid.as_raw() as u32)
}

#[cfg(not(unix))]
pub fn process_group_id(_pid: u32) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::{current_effective_uid, process_group_id};

    #[test]
    fn current_effective_uid_returns_stable_value() {
        assert_eq!(current_effective_uid(), current_effective_uid());
    }

    #[cfg(unix)]
    #[test]
    fn process_group_id_returns_current_group_on_unix() {
        assert!(process_group_id(std::process::id()).is_some());
    }

    #[cfg(not(unix))]
    #[test]
    fn process_group_id_returns_none_on_non_unix() {
        assert_eq!(process_group_id(std::process::id()), None);
    }
}
