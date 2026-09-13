//! Built-in commands: one-click buttons for the everyday chores -- update
//! the system, see what fills the disk, which services failed.
//!
//! The buttons sit under the aliases in the sidebar's shell section
//! (`ui::sidebar`). What each one runs depends on the system the tab is
//! on: [`Family`] picks between pacman, apt, dnf and the rest. Locally
//! that comes from `/etc/os-release`, over SSH from a probe the worker
//! runs on login (`ssh::connection`); either can be overridden -- globally
//! in the settings, per host under "Advanced".
//!
//! Confirmation prompts are left alone by default: no `-y`, no
//! `--noconfirm`, so the command itself still asks before it changes
//! anything. Only `commands_assume_yes` in the config adds those flags.
//!
//! Command lines are written here with `sudo ` and a `{yes}` placeholder
//! in them ([`Family::yes`], [`Family::ask`]); [`catalog`] fills both in.

use std::sync::LazyLock;

use crate::i18n::t;

/// The system a command runs on -- really the package manager, since
/// that's what the commands differ in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Family {
    /// Arch, CachyOS, Manjaro, EndeavourOS: pacman.
    Arch,
    /// Debian, Ubuntu, Mint, Pop!_OS: apt.
    Debian,
    /// Fedora, RHEL, Rocky, Alma: dnf.
    Fedora,
    /// openSUSE, SLES: zypper.
    Suse,
    Alpine,
    Void,
    Gentoo,
    NixOs,
    MacOs,
    FreeBsd,
    /// Not recognized (or not looked at yet): only the commands every
    /// system has.
    #[default]
    Unknown,
}

impl Family {
    pub const ALL: [Family; 11] = [
        Family::Arch,
        Family::Debian,
        Family::Fedora,
        Family::Suse,
        Family::Alpine,
        Family::Void,
        Family::Gentoo,
        Family::NixOs,
        Family::MacOs,
        Family::FreeBsd,
        Family::Unknown,
    ];

    /// The value in config.toml and hosts.toml.
    pub fn key(self) -> &'static str {
        match self {
            Self::Arch => "arch",
            Self::Debian => "debian",
            Self::Fedora => "fedora",
            Self::Suse => "suse",
            Self::Alpine => "alpine",
            Self::Void => "void",
            Self::Gentoo => "gentoo",
            Self::NixOs => "nixos",
            Self::MacOs => "macos",
            Self::FreeBsd => "freebsd",
            Self::Unknown => "auto",
        }
    }

    /// A key from a config file; unknown values (and `auto`) mean "detect".
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_lowercase();
        Self::ALL.into_iter().find(|family| family.key() == value)
    }

    /// The distributions this stands for, for the settings' dropdown.
    pub fn label(self) -> String {
        match self {
            Self::Arch => t!("cmd-family-arch"),
            Self::Debian => t!("cmd-family-debian"),
            Self::Fedora => t!("cmd-family-fedora"),
            Self::Suse => t!("cmd-family-suse"),
            Self::Alpine => t!("cmd-family-alpine"),
            Self::Void => t!("cmd-family-void"),
            Self::Gentoo => t!("cmd-family-gentoo"),
            Self::NixOs => t!("cmd-family-nixos"),
            Self::MacOs => t!("cmd-family-macos"),
            Self::FreeBsd => t!("cmd-family-freebsd"),
            Self::Unknown => t!("cmd-family-unknown"),
        }
    }

    /// Systemd is what the service and log commands go through.
    fn systemd(self) -> bool {
        matches!(self, Self::Arch | Self::Debian | Self::Fedora | Self::Suse | Self::NixOs)
    }

    /// GNU coreutils and iproute2, as opposed to the BSD tools on macOS
    /// and FreeBSD.
    fn gnu(self) -> bool {
        !matches!(self, Self::MacOs | Self::FreeBsd)
    }

    /// What `{yes}` becomes once the user allowed running without being
    /// asked.
    fn yes(self) -> &'static str {
        match self {
            Self::Arch => " --noconfirm",
            Self::Debian | Self::Fedora | Self::Suse | Self::Void | Self::FreeBsd => " -y",
            // apk, nix and brew don't ask in the first place, and Portage
            // only asks when told to (see `ask`).
            Self::Alpine | Self::Gentoo | Self::NixOs | Self::MacOs | Self::Unknown => "",
        }
    }

    /// What `{yes}` becomes otherwise -- empty everywhere but Portage,
    /// which needs telling to ask.
    fn ask(self) -> &'static str {
        match self {
            Self::Gentoo => " --ask",
            _ => "",
        }
    }

    /// The line for `command`, still holding `sudo ` and `{yes}`; `None`
    /// where this system has nothing for it.
    fn line(self, command: Builtin) -> Option<&'static str> {
        let line = match command {
            Builtin::Update => match self {
                Self::Arch => "sudo pacman -Syu{yes}",
                Self::Debian => "sudo apt update && sudo apt upgrade{yes}",
                Self::Fedora => "sudo dnf upgrade --refresh{yes}",
                Self::Suse => "sudo zypper refresh && sudo zypper update{yes}",
                Self::Alpine => "sudo apk update && sudo apk upgrade",
                Self::Void => "sudo xbps-install -Su{yes}",
                Self::Gentoo => "sudo emerge --sync && sudo emerge -uDN @world{yes}",
                Self::NixOs => "sudo nixos-rebuild switch --upgrade",
                Self::MacOs => "brew update && brew upgrade",
                Self::FreeBsd => "sudo pkg update && sudo pkg upgrade{yes}",
                Self::Unknown => return None,
            },
            Builtin::Outdated => match self {
                Self::Arch => "pacman -Qu",
                Self::Debian => "apt list --upgradable",
                Self::Fedora => "dnf check-update",
                Self::Suse => "zypper list-updates",
                Self::Alpine => "apk version -l '<'",
                Self::Void => "xbps-install -Sun",
                Self::Gentoo => "emerge -puDN @world",
                Self::NixOs => return None,
                Self::MacOs => "brew outdated",
                Self::FreeBsd => "pkg version -vRL=",
                Self::Unknown => return None,
            },
            Builtin::Cleanup => match self {
                // Orphans first: with none of them `pacman -Rns` would
                // complain about its empty argument list.
                Self::Arch => "pacman -Qdtq | sudo pacman -Rns -{yes}",
                Self::Debian => "sudo apt autoremove{yes} && sudo apt clean",
                Self::Fedora => "sudo dnf autoremove{yes} && sudo dnf clean packages",
                Self::Suse => "sudo zypper clean --all",
                Self::Alpine => "sudo apk cache clean",
                Self::Void => "sudo xbps-remove -Oo{yes}",
                Self::Gentoo => "sudo emerge --depclean{yes} && sudo eclean-dist",
                Self::NixOs => "sudo nix-collect-garbage -d",
                Self::MacOs => "brew cleanup",
                Self::FreeBsd => "sudo pkg autoremove{yes} && sudo pkg clean{yes}",
                Self::Unknown => return None,
            },
            Builtin::DiskFree => {
                if self.gnu() {
                    "df -h -x tmpfs -x devtmpfs -x efivarfs"
                } else {
                    "df -h"
                }
            }
            Builtin::DiskUsage => {
                if self.gnu() {
                    "du -h --max-depth=1 . | sort -h"
                } else {
                    "du -h -d 1 . | sort -h"
                }
            }
            Builtin::Memory => match self {
                Self::MacOs => "vm_stat",
                Self::FreeBsd => "top -b -d 1 | head -n 8",
                _ => "free -h",
            },
            Builtin::Processes => {
                if self.gnu() {
                    "ps -eo pid,user,pcpu,pmem,comm --sort=-pcpu | head -n 16"
                } else {
                    "ps -Ao pid,user,pcpu,pmem,comm -r | head -n 16"
                }
            }
            Builtin::Uptime => "uptime",
            Builtin::FailedServices => {
                if self.systemd() {
                    "systemctl --failed"
                } else {
                    return None;
                }
            }
            Builtin::LogErrors => {
                if self.systemd() {
                    "journalctl -p err -b --no-pager | tail -n 50"
                } else {
                    return None;
                }
            }
            Builtin::Ports => {
                if self.gnu() {
                    "sudo ss -tulpn"
                } else {
                    "sudo netstat -an -p tcp"
                }
            }
            Builtin::Addresses => {
                if self.gnu() {
                    "ip -brief address"
                } else {
                    "ifconfig"
                }
            }
        };
        Some(line)
    }
}

/// The commands themselves. Their order here is the order of the buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Update,
    Outdated,
    Cleanup,
    DiskFree,
    DiskUsage,
    Memory,
    Processes,
    Uptime,
    FailedServices,
    LogErrors,
    Ports,
    Addresses,
}

/// Which heading a command sits under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Packages,
    Disk,
    System,
    Network,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Packages, Group::Disk, Group::System, Group::Network];

    pub fn label(self) -> String {
        match self {
            Self::Packages => t!("cmd-group-packages"),
            Self::Disk => t!("cmd-group-disk"),
            Self::System => t!("cmd-group-system"),
            Self::Network => t!("cmd-group-network"),
        }
    }
}

impl Builtin {
    const ALL: [Builtin; 12] = [
        Builtin::Update,
        Builtin::Outdated,
        Builtin::Cleanup,
        Builtin::DiskFree,
        Builtin::DiskUsage,
        Builtin::Memory,
        Builtin::Processes,
        Builtin::Uptime,
        Builtin::FailedServices,
        Builtin::LogErrors,
        Builtin::Ports,
        Builtin::Addresses,
    ];

    pub fn group(self) -> Group {
        match self {
            Self::Update | Self::Outdated | Self::Cleanup => Group::Packages,
            Self::DiskFree | Self::DiskUsage => Group::Disk,
            Self::Memory | Self::Processes | Self::Uptime | Self::FailedServices | Self::LogErrors => Group::System,
            Self::Ports | Self::Addresses => Group::Network,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Update => t!("cmd-update"),
            Self::Outdated => t!("cmd-outdated"),
            Self::Cleanup => t!("cmd-cleanup"),
            Self::DiskFree => t!("cmd-disk-free"),
            Self::DiskUsage => t!("cmd-disk-usage"),
            Self::Memory => t!("cmd-memory"),
            Self::Processes => t!("cmd-processes"),
            Self::Uptime => t!("cmd-uptime"),
            Self::FailedServices => t!("cmd-failed-services"),
            Self::LogErrors => t!("cmd-log-errors"),
            Self::Ports => t!("cmd-ports"),
            Self::Addresses => t!("cmd-addresses"),
        }
    }
}

/// One ready-to-run command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub builtin: Builtin,
    /// The shell line, sudo and confirmation flags resolved.
    pub line: String,
    /// Whether it changes the system -- those get the warning treatment.
    pub changes: bool,
}

/// What a system looks like to the commands: which one it is, and whether
/// we're already root there (then nothing gets a `sudo` prefix).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct System {
    pub family: Family,
    pub root: bool,
}

/// The system the command buttons build their lines for, and where those
/// would go: the active tab.
#[derive(Clone, Debug, Default)]
pub struct Target {
    /// `None` while an SSH connection hasn't found out yet.
    pub system: Option<System>,
    /// The host, when the active tab is an SSH session.
    pub host: Option<String>,
    /// The system was set in the config or the host's options rather than
    /// detected -- then the hint points at a different place.
    pub configured: bool,
}

/// Every command this system has, in button order. With `assume_yes` the
/// package commands lose their confirmation prompt.
pub fn catalog(system: System, assume_yes: bool) -> Vec<Command> {
    let confirm = if assume_yes { system.family.yes() } else { system.family.ask() };
    Builtin::ALL
        .into_iter()
        .filter_map(|builtin| {
            let line = system.family.line(builtin)?.replace("{yes}", confirm);
            let line = if system.root { line.replace("sudo ", "") } else { line };
            let changes = matches!(builtin, Builtin::Update | Builtin::Cleanup);
            Some(Command { builtin, line, changes })
        })
        .collect()
}

/// What to run on a freshly connected host to find out what it is. One
/// line, so it fits a single exec channel; output is read by [`probe`].
pub const PROBE: &str = "id -u; uname -s; cat /etc/os-release 2>/dev/null";

/// Read [`PROBE`]'s output: the first line is the user id, the second the
/// kernel, the rest `os-release`.
pub fn probe(output: &str) -> System {
    let mut lines = output.lines();
    let root = lines.next().is_some_and(|id| id.trim() == "0");
    let kernel = lines.next().unwrap_or_default().trim().to_lowercase();
    let family = match kernel.as_str() {
        "darwin" => Family::MacOs,
        "freebsd" => Family::FreeBsd,
        _ => from_os_release(&lines.collect::<Vec<_>>().join("\n")),
    };
    System { family, root }
}

/// The local system, as `/etc/os-release` and the effective user id have
/// it. Unreadable or unknown: [`Family::Unknown`]. Read once -- the
/// sidebar asks for it on every frame it draws.
pub fn local() -> System {
    static LOCAL: LazyLock<System> = LazyLock::new(|| {
        let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        // SAFETY: geteuid only reads the process's own id and can't fail.
        let root = unsafe { libc::geteuid() } == 0;
        System { family: from_os_release(&text), root }
    });
    *LOCAL
}

/// `ID` names the distribution, `ID_LIKE` what it's built on -- so a
/// derivative we've never heard of still lands in the right family.
pub fn from_os_release(text: &str) -> Family {
    let value = |key: &str| {
        text.lines()
            .filter_map(|line| line.split_once('='))
            .find(|(name, _)| name.trim() == key)
            .map(|(_, value)| value.trim().trim_matches(['"', '\'']).to_lowercase())
    };
    let by_id = |id: &str| match id {
        "arch" | "archarm" | "cachyos" | "manjaro" | "endeavouros" | "artix" | "garuda" => Some(Family::Arch),
        "debian" | "ubuntu" | "linuxmint" | "pop" | "raspbian" | "devuan" => Some(Family::Debian),
        "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "ol" | "amzn" => Some(Family::Fedora),
        "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" | "sles" | "suse" => Some(Family::Suse),
        "alpine" => Some(Family::Alpine),
        "void" => Some(Family::Void),
        "gentoo" => Some(Family::Gentoo),
        "nixos" => Some(Family::NixOs),
        _ => None,
    };
    let id = value("ID").and_then(|id| by_id(&id));
    // ID_LIKE lists several, closest first.
    id.or_else(|| value("ID_LIKE").and_then(|like| like.split_whitespace().find_map(by_id))).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{Builtin, Family, System, catalog, from_os_release, probe};

    #[test]
    fn reads_os_release() {
        assert_eq!(from_os_release("ID=arch\nNAME=\"Arch Linux\""), Family::Arch);
        assert_eq!(from_os_release("NAME=\"CachyOS\"\nID=cachyos\nID_LIKE=arch"), Family::Arch);
        // Unknown derivative: ID_LIKE decides.
        assert_eq!(from_os_release("ID=frobnix\nID_LIKE=\"ubuntu debian\""), Family::Debian);
        assert_eq!(from_os_release("ID=\"rocky\"\nID_LIKE=\"rhel centos fedora\""), Family::Fedora);
        assert_eq!(from_os_release("ID=opensuse-tumbleweed"), Family::Suse);
        assert_eq!(from_os_release(""), Family::Unknown);
        assert_eq!(from_os_release("ID=plan9"), Family::Unknown);
    }

    #[test]
    fn reads_the_probe() {
        let system = probe("1000\nLinux\nID=debian\nVERSION_ID=\"12\"\n");
        assert_eq!(system, System { family: Family::Debian, root: false });
        let system = probe("0\nLinux\nID=arch\n");
        assert_eq!(system, System { family: Family::Arch, root: true });
        // macOS and FreeBSD have no os-release.
        assert_eq!(probe("501\nDarwin\n").family, Family::MacOs);
        assert_eq!(probe("0\nFreeBSD\n"), System { family: Family::FreeBsd, root: true });
        assert_eq!(probe(""), System::default());
    }

    #[test]
    fn parses_config_values() {
        for family in Family::ALL {
            assert_eq!(Family::parse(family.key()), Some(family));
        }
        assert_eq!(Family::parse(" Debian "), Some(Family::Debian));
        assert_eq!(Family::parse("auto"), Some(Family::Unknown));
        assert_eq!(Family::parse("plan9"), None);
    }

    fn line(system: System, assume_yes: bool, builtin: Builtin) -> Option<String> {
        catalog(system, assume_yes).into_iter().find(|c| c.builtin == builtin).map(|c| c.line)
    }

    #[test]
    fn confirmation_flags_need_the_setting() {
        let arch = System { family: Family::Arch, root: false };
        assert_eq!(line(arch, false, Builtin::Update).unwrap(), "sudo pacman -Syu");
        assert_eq!(line(arch, true, Builtin::Update).unwrap(), "sudo pacman -Syu --noconfirm");
        let debian = System { family: Family::Debian, root: false };
        assert_eq!(line(debian, false, Builtin::Update).unwrap(), "sudo apt update && sudo apt upgrade");
        assert_eq!(line(debian, true, Builtin::Update).unwrap(), "sudo apt update && sudo apt upgrade -y");
        // Portage is the other way round: it only asks when told to.
        let gentoo = System { family: Family::Gentoo, root: false };
        assert!(line(gentoo, false, Builtin::Update).unwrap().ends_with(" --ask"));
        assert!(!line(gentoo, true, Builtin::Update).unwrap().contains("--ask"));
        // No placeholder is ever left behind, whatever the setting.
        for family in Family::ALL {
            for assume_yes in [false, true] {
                for command in catalog(System { family, root: false }, assume_yes) {
                    assert!(!command.line.contains("{yes}"), "{family:?}: {}", command.line);
                }
            }
        }
    }

    #[test]
    fn root_drops_sudo() {
        let system = System { family: Family::Debian, root: true };
        assert_eq!(line(system, false, Builtin::Update).unwrap(), "apt update && apt upgrade");
        for family in Family::ALL {
            for command in catalog(System { family, root: true }, true) {
                assert!(!command.line.contains("sudo "), "{family:?}: {}", command.line);
            }
        }
    }

    #[test]
    fn unknown_systems_keep_the_portable_commands() {
        let commands = catalog(System::default(), false);
        assert!(commands.iter().all(|c| c.builtin.group() != super::Group::Packages));
        assert!(commands.iter().any(|c| c.builtin == Builtin::DiskFree));
        // Nothing that changes the system without knowing what it is.
        assert!(commands.iter().all(|c| !c.changes));
    }

    #[test]
    fn systemd_commands_only_where_there_is_systemd() {
        let with = catalog(System { family: Family::Arch, root: false }, false);
        assert!(with.iter().any(|c| c.builtin == Builtin::FailedServices));
        let without = catalog(System { family: Family::Alpine, root: false }, false);
        assert!(without.iter().all(|c| c.builtin != Builtin::FailedServices));
        assert!(without.iter().all(|c| c.builtin != Builtin::LogErrors));
    }
}
