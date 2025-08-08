use clap::Parser;
use sigma_compiler_core::*;
use std::io;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[clap(version, about, long_about = None)]
struct Args {
    /// show just the output of the transformations (as opposed to the
    /// entire generated code)
    #[arg(short, long)]
    transforms: bool,
}

/// Produce a [`String`] representation of a [`TaggedVarDict`]
fn taggedvardict_to_string(vd: &TaggedVarDict) -> String {
    let scalars_str = vd
        .values()
        .filter_map(|v| match v {
            TaggedIdent::Scalar(ts) => Some(ts.to_string()),
            _ => None,
        })
        .collect::<Vec<String>>()
        .join(", ");
    let points_str = vd
        .values()
        .filter_map(|v| match v {
            TaggedIdent::Point(tp) => Some(tp.to_string()),
            _ => None,
        })
        .collect::<Vec<String>>()
        .join(", ");
    format!("({scalars_str}),\n({points_str}),\n")
}

fn pretty_print(code_str: &str) {
    let parsed_output = syn::parse_file(code_str).unwrap();
    let formatted_output = prettyplease::unparse(&parsed_output);
    println!("{}", formatted_output);
}

fn main() -> ExitCode {
    let args = Args::parse();
    let emit_prover = true;
    let emit_verifier = true;

    let stdin = io::read_to_string(io::stdin()).unwrap();
    let mut spec: SigmaCompSpec = match syn::parse_str(&stdin) {
        Err(_) => {
            eprintln!("Could not parse stdin as a sigma_compiler input");
            return ExitCode::FAILURE;
        }
        Ok(spec) => spec,
    };

    let mut codegen = CodeGen::new(&spec);
    enforce_disjunction_invariant(&mut codegen, &mut spec).unwrap();
    apply_transformations(&mut codegen, &mut spec).unwrap();
    if args.transforms {
        print!("{}", taggedvardict_to_string(&spec.vars));
        spec.statements.dump();
        println!();
        let (prove_code, verify_code, _) = codegen.code_strings();
        pretty_print(&format!("fn prove_fragment() {{ {} }}", prove_code));
        pretty_print(&format!("fn verify_fragment() {{ {} }}", verify_code));
    } else {
        let output = codegen.generate(&mut spec, emit_prover, emit_verifier);
        pretty_print(&output.to_string());
    }

    ExitCode::SUCCESS
}
