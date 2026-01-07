use anyhow::{bail, Result};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PackageManager {
    Brew,
    Npm,
    Cargo,
    Pipx,
    Pip,
    Winget,
}

impl PackageManager {
    fn command(self) -> &'static str {
        match self {
            PackageManager::Brew => "brew",
            PackageManager::Npm => "npm",
            PackageManager::Cargo => "cargo",
            PackageManager::Pipx => "pipx",
            PackageManager::Pip => "pip",
            PackageManager::Winget => "winget",
        }
    }

    fn label(self) -> &'static str {
        self.command()
    }
}

#[derive(Debug, Clone, Copy)]
struct InstallTarget {
    name: &'static str,
    command: &'static str,
    description: &'static str,
}

const INSTALLABLE_AGENTS: &[InstallTarget] = &[
    InstallTarget {
        name: "codex",
        command: "codex",
        description: "OpenAI Codex CLI",
    },
    InstallTarget {
        name: "claude",
        command: "claude",
        description: "Anthropic Claude Code",
    },
    InstallTarget {
        name: "gemini",
        command: "gemini",
        description: "Google Gemini CLI",
    },
    InstallTarget {
        name: "pi",
        command: "pi",
        description: "Pi Coding Agent",
    },
];

#[derive(Debug, Clone, Copy)]
struct InstallMethod {
    agent: &'static str,
    manager: PackageManager,
    program: &'static str,
    args: &'static [&'static str],
}

const INSTALL_METHODS: &[InstallMethod] = &[
    InstallMethod {
        agent: "codex",
        manager: PackageManager::Npm,
        program: "npm",
        args: &["install", "-g", "@openai/codex"],
    },
    InstallMethod {
        agent: "claude",
        manager: PackageManager::Npm,
        program: "npm",
        args: &["install", "-g", "@anthropic-ai/claude-code"],
    },
    InstallMethod {
        agent: "gemini",
        manager: PackageManager::Npm,
        program: "npm",
        args: &["install", "-g", "@google/gemini-cli"],
    },
    InstallMethod {
        agent: "pi",
        manager: PackageManager::Npm,
        program: "npm",
        args: &["install", "-g", "@mariozechner/pi-coding-agent"],
    },
];

const PACKAGE_MANAGER_ORDER: &[PackageManager] = &[
    PackageManager::Brew,
    PackageManager::Npm,
    PackageManager::Cargo,
    PackageManager::Pipx,
    PackageManager::Pip,
    PackageManager::Winget,
];

pub fn run_install(agent: Option<String>, all: bool, list: bool) -> Result<()> {
    let managers = detect_package_managers();

    let targets = resolve_targets(agent, all, list)?;

    if list {
        print_install_list(&targets, &managers);
        return Ok(());
    }

    if targets.is_empty() {
        bail!("Specify an agent name or use --all/--list");
    }

    if managers.is_empty() {
        let supported = supported_manager_labels();
        bail!(
            "No supported package manager detected. Install one of: {}.",
            supported.join(", ")
        );
    }

    let mut installed_any = false;
    for target in targets {
        if is_installed(target.command) {
            println!("✓ {} already installed", target.name);
            continue;
        }

        let method = match select_method(target.name, &managers) {
            Some(method) => method,
            None => {
                let supported = supported_managers(target.name);
                if supported.is_empty() {
                    bail!("No install method available for {}", target.name);
                }
                bail!(
                    "No supported package manager found for {}. Requires: {}",
                    target.name,
                    supported.join(", ")
                );
            }
        };

        println!(
            "Installing {} via {}...",
            target.name,
            method.manager.label()
        );
        println!("> {}", format_command(method));

        let status = Command::new(method.program).args(method.args).status()?;
        if !status.success() {
            bail!("Install command failed for {}", target.name);
        }
        println!("✓ {} installed", target.name);
        installed_any = true;
    }

    if all && !installed_any {
        println!("All installable agents are already installed.");
    }

    Ok(())
}

fn detect_package_managers() -> Vec<PackageManager> {
    let supported: std::collections::HashSet<PackageManager> =
        INSTALL_METHODS.iter().map(|m| m.manager).collect();
    PACKAGE_MANAGER_ORDER
        .iter()
        .copied()
        .filter(|manager| supported.contains(manager) && which::which(manager.command()).is_ok())
        .collect()
}

fn resolve_targets(
    agent: Option<String>,
    all: bool,
    list: bool,
) -> Result<Vec<&'static InstallTarget>> {
    if let Some(agent) = agent {
        let agent = agent.to_lowercase();
        if let Some(target) = find_target(&agent) {
            return Ok(vec![target]);
        }
        bail!(
            "Unknown agent '{}'. Supported agents: {}",
            agent,
            INSTALLABLE_AGENTS
                .iter()
                .map(|t| t.name)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    if all || list {
        return Ok(INSTALLABLE_AGENTS.iter().collect());
    }

    Ok(Vec::new())
}

fn find_target(name: &str) -> Option<&'static InstallTarget> {
    INSTALLABLE_AGENTS.iter().find(|t| t.name == name)
}

fn is_installed(command: &str) -> bool {
    which::which(command).is_ok()
}

fn supported_managers(agent: &str) -> Vec<&'static str> {
    let mut managers: Vec<&'static str> = INSTALL_METHODS
        .iter()
        .filter(|m| m.agent == agent)
        .map(|m| m.manager.label())
        .collect();
    managers.sort();
    managers.dedup();
    managers
}

fn supported_manager_labels() -> Vec<&'static str> {
    let mut managers: Vec<&'static str> =
        INSTALL_METHODS.iter().map(|m| m.manager.label()).collect();
    managers.sort();
    managers.dedup();
    managers
}

fn select_method(agent: &str, managers: &[PackageManager]) -> Option<&'static InstallMethod> {
    for manager in managers {
        if let Some(method) = INSTALL_METHODS
            .iter()
            .find(|m| m.agent == agent && m.manager == *manager)
        {
            return Some(method);
        }
    }
    None
}

fn format_command(method: &InstallMethod) -> String {
    let mut parts = Vec::with_capacity(1 + method.args.len());
    parts.push(method.program);
    parts.extend(method.args.iter().copied());
    parts.join(" ")
}

fn print_install_list(targets: &[&InstallTarget], managers: &[PackageManager]) {
    if managers.is_empty() {
        println!("Detected package managers: none");
    } else {
        let detected = managers
            .iter()
            .map(|m| m.label())
            .collect::<Vec<_>>()
            .join(", ");
        println!("Detected package managers: {}", detected);
    }

    println!();
    println!("Install commands:");
    for target in targets {
        if let Some(method) = select_method(target.name, managers) {
            println!(
                "  {:<8} {} ({})",
                target.name,
                format_command(method),
                method.manager.label()
            );
        } else {
            let supported = supported_managers(target.name);
            if supported.is_empty() {
                println!("  {:<8} (no install method available)", target.name);
            } else {
                println!("  {:<8} (requires: {})", target.name, supported.join(", "));
            }
        }
    }

    println!();
    println!("Installable agents:");
    for target in targets {
        println!("  {:<8} {}", target.name, target.description);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_method_prefers_order() {
        let managers = vec![PackageManager::Npm, PackageManager::Cargo];
        let method = select_method("codex", &managers).unwrap();
        assert_eq!(method.program, "npm");
    }

    #[test]
    fn test_supported_managers_list() {
        let managers = supported_managers("codex");
        assert!(managers.contains(&"npm"));
    }
}
