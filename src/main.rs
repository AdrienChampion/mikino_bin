#![allow(dead_code)]

mikino_api::prelude!();

use check::{BaseRes, CheckRes, StepRes};
use trans::Sys;

use ansi_term::{Colour, Style};

#[macro_export]
macro_rules! prelude {
    {} => { use crate::prelude::*; };
    { pub } => { pub use crate::prelude::*; };
}

pub mod mode;

use mode::Mode;

/// Entry point.
pub fn main() {
    Run::new().launch()
}

/// Post-run structure.
pub struct PostRun<'sys> {
    pub env: Run,
    pub base: BaseRes<'sys>,
    pub step: StepRes<'sys>,
}

/// Check environment.
pub struct Check<'env> {
    /// Run env.
    pub env: &'env Run,
    /// System to check.
    pub sys: Sys,
    /// Optional SMT log directory.
    pub smt_log_dir: Option<String>,
}
impl<'env> Deref for Check<'env> {
    type Target = Styles;
    fn deref(&self) -> &Styles {
        self.env.deref()
    }
}
impl<'env> Check<'env> {
    /// Constructor.
    pub fn new(env: &'env Run, input: &str, smt_log_dir: &Option<String>) -> Res<Self> {
        use std::{fs::OpenOptions, io::Read};

        let smt_log_dir = smt_log_dir.clone();
        let mut file = OpenOptions::new().read(true).open(input)?;

        let mut txt = String::new();
        file.read_to_string(&mut txt)?;

        let sys = parse::Parser::new(&txt).sys()?;
        if env.verb >= 3 {
            println!("|===| Parsing {}:", env.styles.green.paint("successful"));
            for line in sys.to_ml_string().lines() {
                println!("| {}", line)
            }
            println!("|===|");
            println!()
        }

        Ok(Self {
            env,
            sys,
            smt_log_dir,
        })
    }

    /// Attemps to prove the POs on a system.
    pub fn run(&self) -> Res<(BaseRes, StepRes)> {
        let base_res = self.base_check()?;
        let step_res = self.step_check()?;

        println!("|===| {} attempt result", self.bold.paint("Induction"));

        if base_res.has_falsifications() {
            println!(
                "| - the following PO(s) are {} in the initial state(s)",
                self.red.paint("falsifiable")
            );
            for (po, _) in base_res.cexs.iter() {
                println!("|   `{}`", self.red.paint(*po))
            }
        } else {
            println!(
                "| - all POs {} in the initial state(s)",
                self.green.paint("hold")
            );
        }

        println!("|");

        if step_res.has_falsifications() {
            println!(
                "| - the following PO(s) are {} (not preserved by the transition relation)",
                self.red.paint("not inductive")
            );
            for (po, _) in step_res.cexs.iter() {
                println!("|   `{}`", self.red.paint(*po))
            }
        } else {
            println!(
                "| - all POs are {} (preserved by the transition relation)",
                self.green.paint("inductive")
            );
        }

        println!("|");

        if !base_res.has_falsifications() && !step_res.has_falsifications() {
            println!(
                "| - system is {}, all reachable states verify the PO(s)",
                self.green.paint("safe")
            )
        } else if base_res.has_falsifications() {
            println!(
                "| - system is {}, some PO(s) are falsified in the initial state(s)",
                self.red.paint("unsafe")
            );
            if self.env.verb == 0 {
                println!(
                    "|   (run again without `{}` to see counterexamples)",
                    self.bold.paint("-q")
                )
            }
        } else if step_res.has_falsifications() {
            println!(
                "| - system {}, some PO(s) are {}",
                self.red.paint("might be unsafe"),
                self.red.paint("not inductive"),
            );
            if self.env.verb == 0 {
                println!(
                    "|   (run again without `{}` to see counterexamples)",
                    self.bold.paint("-q")
                )
            }
        }

        if (base_res.has_falsifications() || step_res.has_falsifications())
            && base_res
                .okay
                .iter()
                .any(|b_ok_po| step_res.okay.iter().any(|s_ok_po| b_ok_po == s_ok_po))
        {
            println!("|");
            println!(
                "| - the following PO(s) {} in the initial state(s) and are {}",
                self.green.paint("hold"),
                self.green.paint("inductive")
            );
            println!(
                "|   and thus {} in all reachable states of the system:",
                self.green.paint("hold")
            );

            for po in base_res.okay.intersection(&step_res.okay) {
                println!("|   `{}`", self.green.paint(*po))
            }
        }

        println!("|===|");

        Ok((base_res, step_res))
    }

    /// Runs BMC.
    pub fn bmc(&self, max: Option<usize>, base: &BaseRes, step: Option<&StepRes>) -> Res<()> {
        let bmc_res = if let Some(step) = step {
            base.merge_base_with_step(step)
                .chain_err(|| "during base/step result merge for BMC")?
        } else {
            base.as_inner().clone().into()
        };
        if bmc_res.all_falsified() {
            return Ok(());
        }

        println!(
            "running {}, looking for falsifications for {} PO(s)...",
            self.bold.paint("BMC"),
            bmc_res.okay.len()
        );

        let mut bmc = check::Bmc::new(
            &self.sys,
            &self.env.z3_cmd,
            self.smt_log_dir.as_ref(),
            bmc_res,
        )?;
        let mut falsified = Set::new();

        while !bmc.is_done() && max.map(|max| max >= bmc.next_check_step()).unwrap_or(true) {
            let depth_str = bmc.next_check_step().to_string();
            if self.env.verb > 0 {
                println!(
                    "checking for falsifications at depth {}",
                    self.env.styles.under.paint(&depth_str)
                );
            }

            let new_falsifications = bmc.next_check().chain_err(|| {
                format!(
                    "while checking for falsifications at depth {} in BMC",
                    self.env.styles.under.paint(&depth_str)
                )
            })?;

            if new_falsifications {
                for (po, cex) in bmc.res().cexs.iter() {
                    let is_new = falsified.insert(po.to_string());
                    if is_new {
                        println!(
                            "found a {} at depth {}:",
                            self.red.paint("falsification"),
                            self.env.styles.bold.paint(&depth_str)
                        );
                        self.present_cex(&self.sys, po, cex, true)?
                    }
                }
            }
        }

        let bmc_res = bmc.destroy()?;

        if self.env.verb > 0 || !bmc_res.cexs.is_empty() {
            println!()
        }

        println!("|===| {} result", self.bold.paint("Bmc"));
        if !bmc_res.okay.is_empty() {
            println!(
                "| - could {} find falsifications for the following PO(s)",
                self.bold.paint("not")
            );
            for po in &bmc_res.okay {
                println!("|   `{}`", self.bold.paint(po as &str))
            }
        }
        if !bmc_res.okay.is_empty() && !bmc_res.cexs.is_empty() {
            println!("|")
        }
        if !bmc_res.cexs.is_empty() {
            println!(
                "| - found a {} for the following PO(s)",
                self.red.paint("falsification")
            );
            for po in bmc_res.cexs.keys() {
                println!("|   `{}`", self.red.paint(*po))
            }
        }
        println!("|");
        if !base.cexs.is_empty() || !bmc_res.cexs.is_empty() {
            println!("| - system is {}", self.red.paint("unsafe"))
        } else {
            println!("| - system {}", self.red.paint("might be unsafe"),);
            println!(
                "|   no falsification in {} was found for some POs",
                self.bold.paint(format!(
                    "{} step(s) or less",
                    max.expect("[fatal] cannot have BMC with no max end with unfalsified POs"),
                )),
            );
        }
        println!("|===|");

        Ok(())
    }

    /// Performs the base check.
    pub fn base_check(&self) -> Res<BaseRes> {
        if self.env.verb > 0 {
            println!("checking {} case...", self.under.paint("base"))
        }
        let mut base_checker =
            check::Base::new(&self.sys, &self.env.z3_cmd, self.smt_log_dir.as_ref())
                .chain_err(|| "during base checker creation")?;
        let res = base_checker.check().chain_err(|| "during base check")?;
        if self.env.verb > 0 {
            if !res.has_falsifications() {
                println!(
                    "{}: all PO(s) {} in the {} state",
                    self.green.paint("success"),
                    self.green.paint("hold"),
                    self.under.paint("base"),
                )
            } else {
                println!(
                    "{}: the following PO(s) {} in the {} state:",
                    self.red.paint("failed"),
                    self.red.paint("do not hold"),
                    self.under.paint("step")
                );
                self.present_base_cexs(&self.sys, &res)?
            }
            println!()
        }
        Ok(res)
    }

    /// Performs the step check.
    pub fn step_check(&self) -> Res<StepRes> {
        if self.env.verb > 0 {
            println!("checking {} case...", self.under.paint("step"))
        }
        let mut step_checker =
            check::Step::new(&self.sys, &self.env.z3_cmd, self.smt_log_dir.as_ref())
                .chain_err(|| "during step checker creation")?;
        let res = step_checker.check().chain_err(|| "during step check")?;
        if self.env.verb > 0 {
            if !res.has_falsifications() {
                println!(
                    "{}: all PO(s) are {}",
                    self.green.paint("success"),
                    self.green.paint("inductive")
                )
            } else {
                println!(
                    "{}: the following PO(s) are {}:",
                    self.red.paint("failed"),
                    self.red.paint("not inductive"),
                );
                self.present_step_cexs(&self.sys, &res)?
            }
            println!()
        }
        Ok(res)
    }

    pub fn present_base_cexs(&self, sys: &trans::Sys, res: &BaseRes) -> Res<()> {
        self.present_cexs(sys, res, true)
    }
    pub fn present_step_cexs(&self, sys: &trans::Sys, res: &StepRes) -> Res<()> {
        self.present_cexs(sys, res, false)
    }
    pub fn present_cexs<'sys, R: Deref<Target = CheckRes<'sys>>>(
        &self,
        sys: &trans::Sys,
        res: &R,
        is_base: bool,
    ) -> Res<()> {
        for (po, cex) in res.cexs.iter() {
            self.present_cex(sys, *po, cex, is_base)?
        }
        Ok(())
    }
    pub fn present_cex(
        &self,
        sys: &trans::Sys,
        po: &str,
        cex: &check::cexs::Cex,
        is_base: bool,
    ) -> Res<()> {
        let max_id_len = sys.decls().max_id_len();
        let def = sys
            .po_s()
            .get(po)
            .ok_or_else(|| format!("failed to retrieve definition for PO `{}`", po))?;
        println!(
            "- `{}` = {}",
            self.red.paint(po),
            self.bold.paint(format!("{}", def))
        );
        for (step, values) in &cex.trace {
            let step_str = if is_base {
                format!("{}", self.under.paint(step.to_string()))
            } else {
                let mut step_str = format!("{}", self.under.paint("k"));
                if *step > 0 {
                    step_str = format!("{}{}", step_str, self.under.paint(format!(" + {}", step)))
                }
                step_str
            };
            println!("  |=| Step {}", step_str);
            for (var, cst) in values {
                let var_str = format!("{: >1$}", var.id(), max_id_len);
                println!("  | {} = {}", self.bold.paint(var_str), cst)
            }
        }
        println!("  |=|");
        Ok(())
    }
}

/// Returns an error if the input string is not a valid integer.
///
/// Used by CLAP.
pub fn validate_int(s: String) -> Result<(), String> {
    macro_rules! abort {
        () => {
            return Err(format!("expected integer, found `{}`", s));
        };
    }
    if s != "0" {
        for (idx, char) in s.chars().enumerate() {
            if idx == 0 {
                if !char.is_numeric() || char == '0' {
                    abort!()
                }
            } else {
                if !char.is_numeric() {
                    abort!()
                }
            }
        }
    }
    Ok(())
}

/// Run environment.
pub struct Run {
    /// Output styles (for coloring).
    pub styles: Styles,
    /// Verbosity.
    pub verb: usize,
    /// Z3 command.
    pub z3_cmd: String,
    /// Run mode.
    pub mode: Mode,
}
impl Deref for Run {
    type Target = Styles;
    fn deref(&self) -> &Styles {
        &self.styles
    }
}
impl Run {
    /// Constructor, handles CLAP.
    pub fn new() -> Self {
        use clap::*;
        let app = clap::App::new("mikino")
            .version(crate_version!())
            .author(crate_authors!())
            .about(
                "A minimal induction engine for transition systems. \
                See the `demo` subcommand if you are just starting out.",
            )
            .args(&[
                Arg::with_name("NO_COLOR")
                    .long("no_color")
                    .help("Deactivates colored output"),
                Arg::with_name("VERB")
                    .short("v")
                    .multiple(true)
                    .help("Increases verbosity"),
                Arg::with_name("Z3_CMD")
                    .long("z3_cmd")
                    .takes_value(true)
                    .default_value("z3")
                    .help("specifies the command to run Z3"),
                Arg::with_name("QUIET")
                    .short("q")
                    .help("Quiet output, only shows the final result (/!\\ hides counterexamples)"),
            ])
            .subcommands(mode::Mode::subcommands())
            .setting(AppSettings::SubcommandRequiredElseHelp)
            .setting(AppSettings::ColorAuto);

        let matches = app.get_matches();
        let color = matches.occurrences_of("NO_COLOR") == 0;
        let verb = ((matches.occurrences_of("VERB") + 1) % 4) as usize;
        let quiet = matches.occurrences_of("QUIET") > 0;
        let z3_cmd = matches
            .value_of("Z3_CMD")
            .expect("argument with default value")
            .into();
        let verb = if quiet {
            0
        } else if verb > 4 {
            4
        } else {
            verb
        };

        let mode = mode::Mode::from_clap(&matches).expect("[clap] could not recognize mode");

        Self {
            styles: Styles::new(color),
            verb,
            z3_cmd,
            mode,
        }
    }

    /// String representation of the demo system.
    pub const DEMO_SYS: &'static str = include_str!("../rsc/demo.rs");

    fn pretty_error(&self, e: &(dyn std::error::Error + 'static)) -> String {
        if let Some(e) = e.downcast_ref::<Error>() {
            match e.kind() {
                ErrorKind::ParseErr(row, col, line, msg) => {
                    let (row_str, col_str) = ((row + 1).to_string(), (col + 1).to_string());
                    let offset = {
                        let mut offset = 0;
                        let mut cnt = 0;
                        for c in line.chars() {
                            if cnt < *col {
                                offset += 1;
                                cnt += c.len_utf8();
                            } else {
                                break;
                            }
                        }
                        offset
                    };
                    let mut s = format!(
                        "parse error at {}:{}\n{} |\n{} | {}",
                        self.bold.paint(&row_str),
                        self.bold.paint(&col_str),
                        " ".repeat(row_str.len()),
                        self.bold.paint(&row_str),
                        line,
                    );
                    s.push_str(&format!(
                        "\n{} | {}{} {}",
                        " ".repeat(row_str.len()),
                        " ".repeat(offset),
                        self.red.paint("^~~~"),
                        self.red.paint(msg),
                    ));

                    s
                }
                _ => e.to_string(),
            }
        } else {
            e.to_string()
        }
    }

    /// Launches whatever the user told us to do.
    pub fn launch(&self) {
        if let Err(e) = self.run() {
            println!("|===| {}", self.red.paint("Error"));
            let mut s = self.pretty_error(&e);

            use std::error::Error;
            let mut source = e.source();
            while let Some(e) = source {
                s = format!("{}\n{}", self.pretty_error(e), s);
                source = e.source()
            }
            for line in s.lines() {
                println!("| {}", line)
            }
            println!("|===|");
        }
    }

    /// Runs the mode.
    pub fn run(&self) -> Res<()> {
        match &self.mode {
            Mode::Check {
                input,
                smt_log,
                induction,
                bmc,
                bmc_max,
            } => {
                if let Some(smt_log) = smt_log {
                    if !std::path::Path::new(smt_log).exists() {
                        std::fs::create_dir_all(smt_log).chain_err(|| {
                            format!("while recursively creating SMT log directory `{}`", smt_log)
                        })?
                    }
                }
                let check = Check::new(self, input, smt_log)?;
                let (base, step) = if *induction {
                    let (base, step) = check.run()?;
                    (base, Some(step))
                } else {
                    (CheckRes::new(&check.sys).into(), None)
                };
                if *bmc {
                    if *induction {
                        println!();
                    }
                    check.bmc(bmc_max.clone(), &base, step.as_ref())?
                }
                Ok(())
            }
            Mode::Demo { target } => self.write_demo(target),
            Mode::Parse { input } => {
                let _check = Check::new(self, input, &None)?;
                Ok(())
            }
        }
    }

    /// Writes the demo file somewhere.
    pub fn write_demo(&self, target: &str) -> Res<()> {
        use std::fs::OpenOptions;
        println!("writing demo system to file `{}`", self.bold.paint(target));
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(target)
            .chain_err(|| format!("while opening file `{}` in write mode", target))?;
        file.write(Self::DEMO_SYS.as_bytes())
            .chain_err(|| format!("while writing demo system to file `{}`", target))?;
        file.flush()
            .chain_err(|| format!("while writing demo system to file `{}`", target))?;
        Ok(())
    }
}

/// Stores the output styles.
pub struct Styles {
    /// Bold style.
    pub bold: Style,
    /// Underlined style.
    pub under: Style,
    /// Red style.
    pub red: Style,
    /// Green style.
    pub green: Style,
}
impl Styles {
    /// Constructor, with colors activated.
    pub fn new_colored() -> Self {
        Self {
            bold: Style::new().bold(),
            under: Style::new().underline(),
            red: Colour::Red.normal(),
            green: Colour::Green.normal(),
        }
    }

    /// Constructor, with color deactivated.
    pub fn new_no_color() -> Self {
        Self {
            bold: Style::new(),
            under: Style::new(),
            red: Style::new(),
            green: Style::new(),
        }
    }

    /// Constructor.
    #[cfg(any(feature = "force-color", not(windows)))]
    pub fn new(color: bool) -> Self {
        if color && atty::is(atty::Stream::Stdout) {
            Self::new_colored()
        } else {
            Self::new_no_color()
        }
    }

    /// Constructor.
    ///
    /// This Windows version always produces colorless style.
    #[cfg(not(any(feature = "force-color", not(windows))))]
    pub fn new(_: bool) -> Self {
        Self {
            bold: Style::new(),
            under: Style::new(),
            red: Style::new(),
            green: Style::new(),
        }
    }
}