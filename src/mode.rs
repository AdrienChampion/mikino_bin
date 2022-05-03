//! Handles run modes.

type App = clap::Command<'static>;
type Arg = clap::Arg<'static>;
type Matches = clap::ArgMatches;

/// Run modes.
#[derive(Debug, Clone)]
pub enum Mode {
    /// Check mode, attempt to prove the `input` system is correct.
    Check {
        input: String,
        smt_log: Option<String>,
        induction: bool,
        bmc: bool,
        bmc_max: Option<usize>,
    },
    /// Script mode, run user's script.
    Script {
        input: String,
        smt_log: Option<String>,
        verb: usize,
    },
    /// Demo mode, generate a demo system to `target` if `check`, otherwise generates a demo script.
    Demo { check: bool, target: String },
    /// Parse mode, does nothing but parse the system.
    Parse { input: String },
}

impl Mode {
    /// Yields all the mode subcommands.
    pub fn subcommands() -> Vec<App> {
        vec![
            cla::check_subcommand(),
            cla::script_subcommand(),
            cla::demo(),
            cla::bmc_subcommand(),
            cla::parse_subcommand(),
        ]
    }

    /// Builds itself from top-level clap matches.
    pub fn from_clap(smt_log: Option<String>, matches: &Matches) -> Option<Mode> {
        let modes = [
            cla::try_check,
            cla::try_script,
            cla::try_bmc,
            cla::try_demo,
            cla::try_parse,
        ];
        for try_mode in &modes {
            let maybe_res = try_mode(smt_log.clone(), matches);
            if maybe_res.is_some() {
                return maybe_res;
            }
        }
        None
    }
}

pub mod cla {
    use super::*;
    use clap::Command;

    pub mod mode {
        pub const CHECK: &str = "check";
        pub const SCRIPT: &str = "script";
        pub const DEMO: &str = "demo";
        pub const BMC: &str = "bmc";
        pub const PARSE: &str = "parse";
    }

    mod arg {
        pub const BMC_KEY: &str = "BMC";
        pub const BMC_MAX_KEY: &str = "BMC_MAX";
        pub const SMT_LOG_KEY: &str = "SMT_LOG";
        pub const SYS_KEY: &str = "SYS_KEY";
        pub const SCRIPT_KEY: &str = "SCRIPT_KEY";
        pub const SCRIPT_VERBOSE_KEY: &str = "SCRIPT_VERBOSE";
        pub const DEMO_SCRIPT_KEY: &str = "DEMO_SCRIPT";
        pub const DEMO_TGT_KEY: &str = "DEMO_TGT";
    }

    fn bmc_max_arg() -> Arg {
        Arg::new(arg::BMC_MAX_KEY)
            .help(
                "Maximum number of transitions ≥ 0 allowed from the \
                initial state(s) in BMC, infinite by default",
            )
            .long("bmc_max")
            .validator(validate_int)
            .value_name("INT")
    }
    /// Yields the BMC max value, if any.
    fn get_bmc_max(matches: &Matches, mut if_present_do: impl FnMut()) -> Option<usize> {
        matches.value_of(arg::BMC_MAX_KEY).map(|val| {
            if_present_do();
            usize::from_str_radix(val, 10)
                .expect(&format!("[clap] unexpected value for BMC max: `{}`", val))
        })
    }

    pub fn smt_log_arg() -> Arg {
        Arg::new(arg::SMT_LOG_KEY)
            .help("Activates SMT logging in the directory specified")
            .long("smt_log")
            .short('l')
            .value_name("DIR")
    }
    pub fn get_smt_log(matches: &Matches) -> Option<String> {
        matches.value_of(arg::SMT_LOG_KEY).map(String::from)
    }

    fn sys_arg() -> Arg {
        Arg::new(arg::SYS_KEY)
            .help("Transition system to analyze (run `mikino demo -h` mode for details)")
            .required(true)
            .value_name("FILE")
    }
    fn get_sys(matches: &Matches) -> String {
        matches
            .value_of(arg::SYS_KEY)
            .expect("[clap] required system argument cannot be absent")
            .into()
    }

    fn script_arg() -> Arg {
        Arg::new(arg::SCRIPT_KEY)
            .help("Hsmt script to run (run `mikino demo -h` mode for details)")
            .required(true)
            .value_name("FILE")
    }
    fn get_script(matches: &Matches) -> String {
        matches
            .value_of(arg::SCRIPT_KEY)
            .expect("[clap] required script argument cannot be absent")
            .into()
    }

    /// Subcommand for the check mode.
    pub fn check_subcommand() -> App {
        Command::new(mode::CHECK)
            .about("Attempts to prove that the input transition system is correct")
            .args(&[
                Arg::new(arg::BMC_KEY)
                    .help(
                        "Activates BMC (Bounded Model-Checking): \
                        looks for a falsification for candidates found to not be inductive",
                    )
                    .long("bmc"),
                bmc_max_arg(),
                smt_log_arg(),
                sys_arg(),
            ])
    }
    pub fn try_check(smt_log: Option<String>, matches: &Matches) -> Option<Mode> {
        let matches = matches.subcommand_matches(mode::CHECK)?;

        let input = get_sys(matches);
        let smt_log = get_smt_log(matches).or(smt_log);

        let mut bmc = matches.is_present(arg::BMC_KEY);
        let bmc_max = get_bmc_max(matches, || bmc = true);

        Some(Mode::Check {
            input,
            smt_log,
            induction: true,
            bmc,
            bmc_max,
        })
    }

    /// Subcommand for the check mode.
    pub fn script_subcommand() -> App {
        Command::new(mode::SCRIPT)
            .about("Runs a hsmt script")
            .args(&[
                script_arg(),
                smt_log_arg(),
                Arg::new(arg::SCRIPT_VERBOSE_KEY)
                    .short('v')
                    .long("verbose")
                    .multiple_occurrences(true)
                    .help("increases script output verbosity"),
            ])
    }
    pub fn try_script(smt_log: Option<String>, matches: &Matches) -> Option<Mode> {
        let matches = matches.subcommand_matches(mode::SCRIPT)?;

        let input = get_script(matches);
        let smt_log = get_smt_log(matches).or(smt_log);
        let verb = matches.occurrences_of(arg::SCRIPT_VERBOSE_KEY) as usize;

        Some(Mode::Script {
            input,
            smt_log,
            verb,
        })
    }

    /// Subcommand for the demo mode.
    pub fn demo() -> App {
        Command::new(mode::DEMO)
            .about(
                "Generates a demo transition system file, \
                recommended if you are just starting out. \
                /!\\ OVERWRITES the target file.\n\n\
                Use `--script` to generate a demo script instead.",
            )
            .args(&[
                Arg::new(arg::DEMO_SCRIPT_KEY)
                    .short('s')
                    .long("script")
                    .help("generate a demo **script**"),
                Arg::new(arg::DEMO_TGT_KEY)
                    .help("Path of the file to write the demo file to")
                    .required(true),
            ])
    }
    pub fn try_demo(_smt_log: Option<String>, matches: &Matches) -> Option<Mode> {
        let matches = matches.subcommand_matches(mode::DEMO)?;
        let target = matches
            .value_of(arg::DEMO_TGT_KEY)
            .expect("[clap]: required argument cannot be absent")
            .into();
        let check = matches.occurrences_of(arg::DEMO_SCRIPT_KEY) == 0;

        Some(Mode::Demo { target, check })
    }

    /// Subcommand for the bmc mode.
    pub fn bmc_subcommand() -> App {
        Command::new(mode::BMC)
            .about(
                "Runs BMC (Bounded Model Checking) without induction. \
            Mikino will search for a falsification for each proof objective.",
            )
            .args(&[bmc_max_arg(), smt_log_arg(), sys_arg()])
    }
    pub fn try_bmc(smt_log: Option<String>, matches: &Matches) -> Option<Mode> {
        let matches = matches.subcommand_matches(mode::BMC)?;
        let bmc_max = get_bmc_max(matches, || ());
        let smt_log = get_smt_log(matches).or(smt_log);
        let input = get_sys(matches);
        let induction = false;
        let bmc = true;
        Some(Mode::Check {
            input,
            bmc,
            bmc_max,
            induction,
            smt_log,
        })
    }

    /// Subcommand for parse mode.
    pub fn parse_subcommand() -> App {
        Command::new(mode::PARSE)
            .about("Parses the input system and exits")
            .arg(sys_arg())
    }
    pub fn try_parse(_smt_log: Option<String>, matches: &Matches) -> Option<Mode> {
        let matches = matches.subcommand_matches(mode::PARSE)?;
        let input = get_sys(matches);
        Some(Mode::Parse { input })
    }

    /// Returns an error if the input string is not a valid integer.
    ///
    /// Used by CLAP.
    pub fn validate_int(s: &str) -> Result<(), String> {
        macro_rules! abort {
            () => {
                return Err(format!("expected integer, found `{}`", s))
            };
        }
        if s != "0" {
            for (idx, char) in s.chars().enumerate() {
                if idx == 0 {
                    if !char.is_numeric() || char == '0' {
                        abort!();
                    }
                } else {
                    if !char.is_numeric() {
                        abort!();
                    }
                }
            }
        }
        Ok(())
    }
}
