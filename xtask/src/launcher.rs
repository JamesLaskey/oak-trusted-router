//
// Copyright 2020 The Project Oak Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

//! Functionality for testing variants of the enclave binary exposed by the launcher.

pub static MOCK_LOOKUP_DATA_PATH: Lazy<PathBuf> =
    Lazy::new(|| workspace_path(&["oak_functions_launcher", "mock_lookup_data"]));
static STAGE_0_DIR: Lazy<PathBuf> = Lazy::new(|| workspace_path(&["stage0"]));
pub static OAK_RESTRICTED_KERNEL_BIN_DIR: Lazy<PathBuf> =
    Lazy::new(|| workspace_path(&["oak_restricted_kernel_bin"]));
static OAK_FUNCTIONS_LAUNCHER_BIN: Lazy<PathBuf> =
    Lazy::new(|| workspace_path(&["target", "debug", "oak_functions_launcher"]));
pub static QUIRK_ECHO_LAUNCHER_BIN: Lazy<PathBuf> =
    Lazy::new(|| workspace_path(&["target", "debug", "quirk_echo_launcher"]));

use crate::{internal::*, workspace_path};
use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};

#[derive(Debug, Display, Clone, PartialEq, EnumIter)]
pub enum LauncherMode {
    /// Virtual launch mode parameterized by the app binary crate name.
    Virtual(String),
    /// Native launch mode parameterized by the app binary crate name.
    Native(String),
}

impl LauncherMode {
    /// Get the crate name of respective enclave binary variant
    pub fn enclave_crate_name(&self) -> String {
        match self {
            LauncherMode::Virtual(s) => s.clone(),
            LauncherMode::Native(s) => s.clone(),
        }
    }

    /// Get the path to the respective enclave binary variant that should be launched
    pub fn enclave_crate_path(&self) -> String {
        workspace_path(&[&self.enclave_crate_name()])
            .to_str()
            .unwrap()
            .to_string()
    }

    /// Get the path to the respective enclave binary variant that should be launched
    pub fn enclave_binary_path(&self) -> String {
        match self {
            LauncherMode::Virtual(_) => workspace_path(&[
                &self.enclave_crate_name(),
                "target",
                "x86_64-unknown-none",
                "debug",
                &self.enclave_crate_name(),
            ])
            .to_str()
            .unwrap()
            .to_string(),
            LauncherMode::Native(_) => format!("./target/debug/{}", self.enclave_crate_name()),
        }
    }

    /// Get the subcommand for launching in this mode
    pub fn variant_subcommand(&self) -> Vec<String> {
        match self {
            LauncherMode::Virtual(_) => vec![
                "virtualized".to_string(),
                format!(
                    "--enclave-binary={}",
                    workspace_path(&[
                        "oak_restricted_kernel_bin",
                        "target",
                        "x86_64-unknown-none",
                        "debug",
                        "oak_restricted_kernel_bin",
                    ])
                    .to_str()
                    .unwrap()
                ),
                format!("--vmm-binary={}", "/usr/bin/qemu-system-x86_64"),
                format!("--app-binary={}", &self.enclave_binary_path()),
                format!(
                    "--bios-binary={}",
                    workspace_path(&[
                        "stage0",
                        "target",
                        "x86_64-unknown-none",
                        "release",
                        "oak_stage0.bin",
                    ])
                    .to_str()
                    .unwrap()
                ),
            ],
            LauncherMode::Native(_) => vec![
                "native".to_string(),
                format!("--enclave-binary={}", &self.enclave_binary_path()),
            ],
        }
    }
}

pub fn build_enclave_binary_variants(opt: &BuildEnclaveBinaryVariantsOpt) -> Step {
    Step::Multiple {
        name: "Build enclave binary variants".to_string(),
        steps: LauncherMode::iter()
            .filter(|v| option_covers_variant(opt, v))
            .map(|v| build_released_binary(&v.to_string(), &v.enclave_crate_path()))
            .collect(),
    }
}

fn option_covers_variant(opt: &BuildEnclaveBinaryVariantsOpt, variant: &LauncherMode) -> bool {
    match &opt.variant {
        None => true,
        Some(var) => match *variant {
            LauncherMode::Native(_) => var == "native",
            LauncherMode::Virtual(_) => var == "virtual",
        },
    }
}

fn build_released_binary(name: &str, directory: &str) -> Step {
    Step::Single {
        name: name.to_string(),
        command: Cmd::new_in_dir("cargo", vec!["build", "--release"], Path::new(directory)),
    }
}

pub fn build_stage0() -> Step {
    Step::Single {
        name: "build stage0".to_string(),
        command: Cmd::new_in_dir(
            "cargo",
            vec![
                "objcopy",
                "--release",
                "--",
                "-O",
                "binary",
                "target/x86_64-unknown-none/release/oak_stage0.bin",
            ],
            STAGE_0_DIR.as_path(),
        ),
    }
}

pub fn build_binary(name: &str, directory: &str) -> Step {
    Step::Single {
        name: name.to_string(),
        command: Cmd::new_in_dir("cargo", vec!["build"], Path::new(directory)),
    }
}

/// Runs the Oak Functions launcher configured with a default Wasm module for key / value lookups
/// and mock lookup data.
pub fn run_oak_functions_launcher_example(
    variant: &LauncherMode,
    wasm_path: &str,
) -> Box<dyn Runnable> {
    let mut args = vec![
        format!("--wasm={}", wasm_path),
        format!("--lookup-data={}", MOCK_LOOKUP_DATA_PATH.to_str().unwrap()),
    ];
    args.append(&mut variant.variant_subcommand());
    Cmd::new(OAK_FUNCTIONS_LAUNCHER_BIN.to_str().unwrap(), args)
}

pub fn run_launcher(launcher_bin: &str, variant: &LauncherMode) -> Box<dyn Runnable> {
    let mut args = vec![];
    args.append(&mut variant.variant_subcommand());
    Cmd::new(launcher_bin, args)
}
