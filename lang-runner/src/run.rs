use std::{path::PathBuf, process::Stdio, time::Duration};

use async_process::Command;
use common::{
    langs::{Lang, LANGS},
    RunLangOutput,
};
use futures_util::AsyncWriteExt;
use serde::Serialize;

use crate::{
    cachemap::CacheMap,
    error::{RunLangError, RunProcessError},
    parse_output::parse_judge_result_from_stream,
    Message,
};

const MAX_CONCURRENT_RUNS: usize = 4;

static RUNS_SEMAPHORE: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(MAX_CONCURRENT_RUNS);

async fn install_plugin(lang: &Lang) -> Result<CacheMap<String, ()>, RunProcessError> {
    println!("Installing language version {}", lang.name);
    let plugin_install_output = Command::new("asdf")
        .args(["plugin", "add", lang.name, lang.plugin])
        .stderr(Stdio::inherit())
        .status()
        .await?;
    if !plugin_install_output.success() {
        return Err(RunProcessError::NonZeroStatusCode(
            plugin_install_output.code(),
        ));
    }
    Ok(CacheMap::new())
}

async fn install_language_version(lang: &Lang, version: &str) -> Result<(), RunProcessError> {
    println!("Installing language version {} {}", lang.name, version);
    let status = Command::new("asdf")
        .args(["install", lang.name, version])
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        return Err(RunProcessError::NonZeroStatusCode(status.code()));
    }
    Ok(())
}

async fn install_lang(
    lang_name: String,
    version: &str,
    versions: &CacheMap<String, CacheMap<String, ()>>,
) -> Result<(), RunProcessError> {
    let lang = LANGS.iter().find(|k| k.name == lang_name).unwrap();

    let lang_version_token = versions.get(lang.name.to_owned());
    let lang_versions = lang_version_token
        .get_or_try_init(|| install_plugin(lang))
        .await?;

    let specific_version_token = lang_versions.get(version.to_owned());

    let _specific_version = specific_version_token
        .get_or_try_init(|| install_language_version(lang, version))
        .await?;

    Ok(())
}

async fn get_lang_directory(lang: &Lang, version: &str) -> Result<PathBuf, RunProcessError> {
    let lang_folder = Command::new("asdf")
        .args(["where", lang.name, version])
        .stderr(Stdio::inherit())
        .output()
        .await?;
    if !lang_folder.status.success() {
        return Err(RunProcessError::NonZeroStatusCode(
            lang_folder.status.code(),
        ));
    }

    let buff = PathBuf::from(String::from_utf8(lang_folder.stdout).unwrap().trim());
    Ok(buff)
}

async fn run_lang(
    lang_name: &str,
    version: &str,
    code: &str,
    judge: &str,
    judge_lang: &str,
    judge_version: &str,
) -> Result<RunLangOutput, RunProcessError> {
    let lang = LANGS.iter().find(|k| k.name == lang_name).unwrap();
    let judge_lang = LANGS.iter().find(|k| k.name == judge_lang).unwrap();

    let code_lang_folder = get_lang_directory(lang, version).await?;
    let judge_lang_folder = get_lang_directory(judge_lang, judge_version).await?;

    let mut command = Command::new("bwrap");
    command
        .args([
            "--die-with-parent",
            "--ro-bind",
            "/proc/self",
            "/proc/self",
            "--ro-bind",
            "/bin",
            "/bin",
            "--chdir",
            "/",
            "--ro-bind",
            "/lib64",
            "/lib64",
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind",
            "/lib",
            "/lib",
            "--ro-bind",
            "/etc",
            "/etc",
            "--ro-bind",
            "/etc/alternatives",
            "/etc/alternatives",
            "--tmpfs",
            "/tmp",
            "--tmpfs",
            "/home/yq",
            "--setenv",
            "HOME",
            "/home/yq",
        ])
        .args(["--ro-bind"])
        .arg(code_lang_folder)
        .args(["/lang"])
        .args(["--ro-bind"])
        .arg(judge_lang_folder)
        .arg("/judge")
        .args(["--ro-bind", "/scripts", "/scripts"])
        .args(["--unshare-all", "--new-session"]);

    for (key, value) in judge_lang.env {
        command.args(["--setenv", *key, *value]);
    }

    command
        .args(
            judge_lang
                .run_command
                .iter()
                .map(|k| {
                    k.replace("${LANG_LOCATION}", "/judge")
                        .replace("${FILE_LOCATION}", "/scripts/runner.ts")
                })
                .collect::<Vec<_>>(),
        )
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .stderr(Stdio::piped());

    // .args([&format!("/lang/{}", lang.bin_location), code as &str, judge]);

    let mut child = command.spawn()?;
    let Some(stdin) = &mut child.stdin else {
        panic!("Child stdin should exist");
    };

    #[derive(Serialize)]
    struct RunnerInput<'a> {
        lang: &'a Lang,
        code: &'a str,
        judge: &'a str,
    }

    let data = serde_json::to_string(&RunnerInput { lang, code, judge })
        .map_err(RunProcessError::SerializationFailed)?;
    stdin.write_all(data.as_bytes()).await?;
    stdin.close().await?;

    let judge_result = tokio::spawn(parse_judge_result_from_stream(
        child
            .stdout
            .take()
            .expect("The child stdout is already consumed"),
    ));
    let id = child.id();

    let timed_out = tokio::select! {
        _status = child.status() => {
            println!("Child finished normally {id}");
            false
        }
        _timeout = tokio::time::sleep(Duration::from_secs(3)) => {
            child.kill().unwrap();
            eprintln!("Timed out {id}");
            true
        }
    };
    eprintln!("Awaiting output");
    let output = child.output().await?;

    eprintln!("Output Awaited");

    let mut stderr = output.stderr;
    stderr.truncate(1000);

    Ok(RunLangOutput {
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        tests: judge_result.await.unwrap(),
        timed_out,
    })
}

pub async fn process_message(
    message: Message,
    lang_versions: &CacheMap<String, CacheMap<String, ()>>,
) -> Result<RunLangOutput, RunLangError> {
    // Runner Lang
    install_lang("deno".to_owned(), "2.0.4", lang_versions)
        .await
        .map_err(RunLangError::PluginInstallFailure)?;

    install_lang(message.lang.clone(), &message.version, lang_versions)
        .await
        .map_err(RunLangError::PluginInstallFailure)?;

    let _semaphore = RUNS_SEMAPHORE
        .acquire()
        .await
        .map_err(|e| RunLangError::SemaphoreError(e))?;
    let output = run_lang(
        &message.lang,
        &message.version,
        &message.code,
        &message.judge,
        "deno",
        "2.0.4",
    )
    .await
    .map_err(RunLangError::RunLangError)?;
    Ok(output)
}

async fn get_versions_for_language(line: &str) -> (String, CacheMap<String, ()>) {
    let parts = line.split_ascii_whitespace().collect::<Vec<_>>();
    let Some(name) = parts.first() else {
        panic!("bad output from asdf plugin list: {line} {parts:?}")
    };

    let versions = Command::new("asdf")
        .args(["list", name])
        .output()
        .await
        .unwrap();

    if !versions.status.success() {
        println!("Finding versions failed");
    }

    return (
        (*name).to_owned(),
        String::from_utf8(versions.stdout)
            .unwrap()
            .lines()
            .map(|k| (k.trim().to_owned(), ()))
            .collect::<CacheMap<_, ()>>(),
    );
}

pub async fn get_lang_versions() -> CacheMap<String, CacheMap<String, ()>> {
    let output = Command::new("asdf")
        .args(["plugin", "list"])
        .output()
        .await
        .unwrap();
    if !output.status.success() {
        panic!("Finding the list of plugins failed");
    }

    let output_text = String::from_utf8(output.stdout).unwrap();
    futures_util::future::join_all(output_text.lines().map(get_versions_for_language))
        .await
        .into_iter()
        .collect()
}
