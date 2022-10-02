use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::io::Write;
use std::path::Path;

type IoResult = std::io::Result<()>;

macro_rules! push {
    ($inputs:ident; $gdscript_ty:ident, $rust_ty:ty, $val:expr) => {
        push!($inputs; $gdscript_ty, $rust_ty, $val, $val);
    };

    ($inputs:ident; $gdscript_ty:ident, $rust_ty:ty, $gdscript_val:expr, $rust_val:expr) => {
        $inputs.push(Input {
            ident: stringify!($rust_ty).to_lowercase(),
            gdscript_ty: stringify!($gdscript_ty),
            gdscript_val: stringify!($gdscript_val),
            rust_ty: quote! { $rust_ty },
            rust_val: quote! { $rust_val },
        });
    };
}

// Edit this to change involved types
fn collect_inputs() -> Vec<Input> {
    let mut inputs = vec![];

    // Scalar
    push!(inputs; int, i64, -922337203685477580);
    push!(inputs; int, i32, -2147483648);
    //push!(inputs; int, i16, -32767);
    //push!(inputs; int, i8, -128);
    // push!(inputs; float, f64, 127.83);
    push!(inputs; bool, bool, true);
    push!(inputs; String, GodotString, "hello", "hello".into());

    // Composite
    push!(inputs; int, InstanceId, -1, InstanceId::from_u64(0xFFFFFFFFFFFFFFFF));

    inputs
}

fn main() {
    let inputs = collect_inputs();
    let methods = generate_rust_methods(&inputs);

    let rust_tokens = quote::quote! {
        use gdext_builtin::*;
        use gdext_class::obj::InstanceId;

        #[derive(gdext_macros::GodotClass)]
        #[godot(init)]
        struct GenFfi {}

        #[gdext_macros::godot_api]
        impl GenFfi {
            #(#methods)*
        }
    };

    let rust_output_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/gen"));
    let godot_input_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../godot/input"));
    let godot_output_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../godot/gen"));

    let rust_file = rust_output_dir.join("gen_ffi.rs");
    let gdscript_template = godot_input_dir.join("GenFfiTests.template.gd");
    let gdscript_file = godot_output_dir.join("GenFfiTests.gd");

    std::fs::create_dir_all(rust_output_dir).expect("create Rust parent dir");
    std::fs::create_dir_all(godot_output_dir).expect("create GDScript parent dir");
    std::fs::write(&rust_file, rust_tokens.to_string()).expect("write to Rust file");
    write_gdscript_code(&inputs, &gdscript_template, &gdscript_file)
        .expect("write to GDScript file");

    println!("cargo:rerun-if-changed={}", gdscript_template.display());

    rustfmt_if_needed(vec![rust_file]);
}

// TODO remove, or remove code duplication with codegen
fn rustfmt_if_needed(out_files: Vec<std::path::PathBuf>) {
    //print!("Format {} generated files...", out_files.len());

    let mut process = std::process::Command::new("rustup");
    process
        .arg("run")
        .arg("stable")
        .arg("rustfmt")
        .arg("--edition=2021");

    for file in out_files {
        //println!("Format {file:?}");
        process.arg(file);
    }

    match process.output() {
        Ok(_) => println!("Done."),
        Err(err) => {
            println!("Failed.");
            println!("Error: {}", err);
        }
    }
}

struct Input {
    ident: String,
    gdscript_ty: &'static str,
    gdscript_val: &'static str,
    rust_ty: TokenStream,
    rust_val: TokenStream,
}

fn generate_rust_methods(inputs: &Vec<Input>) -> Vec<TokenStream> {
    inputs
        .iter()
        .map(|input| {
            let Input {
                ident,
                rust_ty,
                rust_val,
                ..
            } = input;

            let return_method = format_ident!("return_{}", ident);
            let accept_method = format_ident!("accept_{}", ident);
            let mirror_method = format_ident!("mirror_{}", ident);

            quote! {
                #[godot]
                fn #return_method(&self) -> #rust_ty {
                    #rust_val
                }

                #[godot]
                fn #accept_method(&self, i: #rust_ty) -> bool {
                    i == #rust_val
                }

                #[godot]
                fn #mirror_method(&self, i: #rust_ty) -> #rust_ty {
                    i
                }
            }
        })
        .collect()
}

// ----------------------------------------------------------------------------------------------------------------------------------------------
// GDScript templating and generation

fn write_gdscript_code(
    inputs: &Vec<Input>,
    in_template_path: &Path,
    out_file_path: &Path,
) -> IoResult {
    let template = std::fs::read_to_string(in_template_path)?;
    let mut file = std::fs::File::create(out_file_path)?;

    // let (mut last_start, mut prev_end) = (0, 0);
    let mut last = 0;

    let ranges = find_repeated_ranges(&template);
    dbg!(&ranges);
    for m in ranges {
        file.write_all(&template[last..m.before_start].as_bytes())?;

        replace_parts(&template[m.start..m.end], inputs, |replacement| {
            file.write_all(replacement.as_bytes())?;
            Ok(())
        })?;

        last = m.after_end;
    }
    file.write_all(&template[last..].as_bytes())?;

    Ok(())
}

fn replace_parts(
    repeat_part: &str,
    inputs: &Vec<Input>,
    mut visitor: impl FnMut(&str) -> IoResult,
) -> IoResult {
    for input in inputs {
        let Input {
            ident,
            gdscript_ty,
            gdscript_val,
            ..
        } = input;

        let replaced = repeat_part
            .replace("IDENT", &ident)
            .replace("TYPE", gdscript_ty)
            .replace("VAL", &gdscript_val.to_string());

        visitor(&replaced)?;
    }

    Ok(())
}

fn find_repeated_ranges(entire: &str) -> Vec<Match> {
    const START_PAT: &'static str = "#(";
    const END_PAT: &'static str = "#)";

    let mut search_start = 0;
    let mut found = vec![];
    loop {
        if let Some(start) = entire[search_start..].find(START_PAT) {
            let before_start = search_start + start;
            let start = before_start + START_PAT.len();
            if let Some(end) = entire[start..].find(END_PAT) {
                let end = start + end;
                let after_end = end + END_PAT.len();

                println!("Found {start}..{end}");
                found.push(Match {
                    before_start,
                    start,
                    end,
                    after_end,
                });
                search_start = after_end;
            } else {
                panic!("unmatched start pattern without end");
            }
        } else {
            break;
        }
    }

    found
}

#[derive(Debug)]
struct Match {
    before_start: usize,
    start: usize,
    end: usize,
    after_end: usize,
}