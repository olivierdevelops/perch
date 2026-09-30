//! Thin bridges from each use-case `Impl` to the trait the `cli` crate
//! dispatches through. They only translate signatures (supplying the real
//! stdout/stderr); behaviour lives in the use cases.
use perch_cli as cli;
use perch_cli::Error;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;

fn stdout() -> std::io::Stdout {
    std::io::stdout()
}

pub struct Run(pub perch_runcommand::Impl);
impl cli::RunCommandUseCase for Run {
    fn execute(&self, c: &str, n: &str, a: &[String]) -> Result<(), Error> {
        self.0.execute(c, n, a)
    }
    fn has_command(&self, c: &str, n: &str) -> bool {
        self.0.has_command(c, n)
    }
}

pub struct List(pub perch_listcommands::Impl);
impl cli::ListCommandsUseCase for List {
    fn execute(&self, c: &str) -> Result<(), Error> {
        self.0.execute(c, &mut stdout())
    }
}

pub struct Init(pub perch_initconfig::Impl);
impl cli::InitConfigUseCase for Init {
    fn execute(&self, p: &str) -> Result<(), Error> {
        self.0.execute(p, &mut stdout()).map_err(Into::into)
    }
}

pub struct Build(pub perch_runbuild::Impl);
impl cli::RunBuildUseCase for Build {
    fn execute(&self, c: &str, a: &[String]) -> Result<(), Error> {
        self.0.execute(c, a, &mut stdout())
    }
}

/// `--build` inside a binary that already embeds a program.
pub struct DisabledBuild;
impl cli::RunBuildUseCase for DisabledBuild {
    fn execute(&self, _: &str, _: &[String]) -> Result<(), Error> {
        Err("--build is disabled in a binary that already embeds a program".into())
    }
}

pub struct Server(pub perch_runserver::Impl);
impl cli::RunServerUseCase for Server {
    fn execute(&self, c: &str, a: &[String]) -> Result<(), Error> {
        self.0.execute(c, a)
    }
}

pub struct Shell(pub perch_runshell::Impl);
impl cli::RunShellUseCase for Shell {
    fn execute(&self, c: &str) -> Result<(), Error> {
        self.0.execute(c)
    }
}

pub struct Validate(pub perch_validate::Impl);
impl cli::ValidateUseCase for Validate {
    fn execute(&self, c: &str) -> Result<(), Error> {
        self.0.execute(c)
    }
}

pub struct CommandHelp(pub perch_commandhelp::Impl);
impl cli::CommandHelpUseCase for CommandHelp {
    fn execute(&self, c: &str, n: &str) -> Result<(), Error> {
        self.0.execute(c, n, &mut stdout())
    }
}

pub struct InstallLsp(pub perch_installlsp::Impl);
impl cli::InstallLSPUseCase for InstallLsp {
    fn execute(&self) -> Result<(), Error> {
        self.0.execute(&mut stdout())
    }
}

pub struct InstallVscode(pub perch_installvscode::Impl);
impl cli::InstallVSCodeUseCase for InstallVscode {
    fn execute(&self) -> Result<(), Error> {
        self.0.execute()
    }
}

pub struct ImportSh(pub perch_importsh::Impl);
impl cli::ImportShUseCase for ImportSh {
    fn execute(&self, s: &str, o: &str) -> Result<(), Error> {
        self.0.execute(s, o, &mut stdout())
    }
}

pub struct Scan(pub perch_scan::Impl);
impl cli::ScanUseCase for Scan {
    fn execute(&self, c: &str) -> Result<(), Error> {
        self.0.execute(c, &mut stdout())
    }
}

pub struct Help(pub perch_help::Impl);
impl cli::HelpUseCase for Help {
    fn execute(&self, topic: &str, as_json: bool) -> Result<(), Error> {
        self.0
            .execute(topic, as_json, &mut stdout(), &mut std::io::stderr())
            .map_err(|e| -> Error { e.to_string().into() })
    }
}

pub struct Test(pub perch_runtests::Impl);
impl cli::TestUseCase for Test {
    fn execute(&self, c: &str, filter: &str, verbose: bool) -> Result<(), Error> {
        self.0.execute(c, filter, verbose, &mut std::io::stderr())
    }
}

pub struct ExportOps(pub perch_exportopscatalog::Impl);
impl cli::ExportOpsCatalogUseCase for ExportOps {
    fn execute(&self, p: &str) -> Result<(), Error> {
        self.0.execute(p)
    }
}

/// Bridges `cli::SimulateEnv` (no use-case imports) to `simulate::SimEnv`.
/// The CLI side can't distinguish an absent list from an empty one, so an
/// empty list/map means "flag not given" (unrestricted).
pub struct Simulate(pub perch_simulate::Impl);
impl cli::SimulateUseCase for Simulate {
    fn execute(
        &self,
        config_path: &str,
        command_name: &str,
        env: &cli::SimulateEnv,
        fixture_path: &str,
        w: &mut dyn Write,
    ) -> Result<(), Error> {
        let opt_vec = |v: &Vec<String>| if v.is_empty() { None } else { Some(v.clone()) };
        let opt_map = |m: &HashMap<String, String>| {
            if m.is_empty() {
                None
            } else {
                Some(m.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<BTreeMap<_, _>>())
            }
        };
        let sim = perch_simulate::SimEnv {
            os: env.os.clone(),
            arch: env.arch.clone(),
            env: opt_map(&env.env),
            env_restrict: env.env_restrict,
            fs_read: opt_vec(&env.fs_read),
            fs_write: opt_vec(&env.fs_write),
            bins: if env.bins.is_empty() {
                None
            } else {
                Some(env.bins.iter().map(|(k, v)| (k.clone(), *v)).collect())
            },
            network: opt_vec(&env.network),
            no_shell: env.no_shell,
            no_subprocess: env.no_subprocess,
            no_network: env.no_network,
            no_write: env.no_write,
        };
        self.0.execute(config_path, command_name, sim, fixture_path, w)
    }
}
