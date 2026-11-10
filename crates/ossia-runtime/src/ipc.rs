use crate::*;
use interprocess::local_socket::{
    tokio::{prelude::*, Listener, Stream},
    GenericFilePath, ListenerOptions,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const MAX_FRAME: usize = 64 * 1024 * 1024;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    pub runtime: RuntimeInfo,
    pub endpoint: String,
}

pub fn endpoint(home: &Path) -> Result<PathBuf> {
    let hash = ContentHash::of(home.as_os_str().as_encoded_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let uid = unsafe { libc::geteuid() };
        let directory = std::env::temp_dir().join(format!("ossia-ipc-{uid}"));
        if !directory.exists() {
            let mut builder = std::fs::DirBuilder::new();
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
            match builder.create(&directory) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(storage(e)),
            }
        }
        let meta = std::fs::symlink_metadata(&directory).map_err(storage)?;
        if !meta.is_dir()
            || meta.file_type().is_symlink()
            || meta.uid() != uid
            || meta.permissions().mode() & 0o077 != 0
        {
            return Err(storage(
                "IPC directory must be private and owned by the current user",
            ));
        }
        // A fixed short path also supports deep library roots on macOS's 104-byte socket limit.
        let directory = if directory.as_os_str().len() > 35 {
            PathBuf::from("/tmp").join(format!("ossia-ipc-{uid}"))
        } else {
            directory
        };
        if !directory.exists() {
            let mut builder = std::fs::DirBuilder::new();
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
            match builder.create(&directory) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(storage(e)),
            }
        }
        let meta = std::fs::symlink_metadata(&directory).map_err(storage)?;
        if !meta.is_dir()
            || meta.file_type().is_symlink()
            || meta.uid() != uid
            || meta.permissions().mode() & 0o077 != 0
        {
            return Err(storage("untrusted IPC directory"));
        }
        Ok(directory.join(format!("{}.sock", &hash.as_str()[..48])))
    }
    #[cfg(windows)]
    {
        Ok(PathBuf::from(format!(r"\\.\pipe\ossia-{}", hash)))
    }
}

pub fn listen(path: &Path) -> Result<Listener> {
    let name = path.to_fs_name::<GenericFilePath>().map_err(storage)?;
    let options = ListenerOptions::new()
        .name(name)
        .reclaim_name(true)
        .try_overwrite(true);
    #[cfg(windows)]
    let options = {
        use interprocess::os::windows::{
            local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor,
        };
        let descriptor = current_user_descriptor()?;
        options.security_descriptor(SecurityDescriptor::deserialize(&descriptor).map_err(storage)?)
    };
    let listener = options
        .create_tokio()
        .map_err(|e| storage(format!("IPC listener at {}: {e}", path.display())))?;
    // The parent is already 0700; chmod by path works on macOS where fchmod(socket) does not.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(storage)?;
    }
    Ok(listener)
}

#[cfg(windows)]
fn current_user_descriptor() -> Result<widestring::U16CString> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree},
        Security::Authorization::ConvertSidToStringSidW,
        Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(storage(std::io::Error::last_os_error()));
        }
        let mut length = 0;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut length);
        let mut buffer = vec![0u64; (length as usize).div_ceil(8)];
        if GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        ) == 0
        {
            CloseHandle(token);
            return Err(storage(std::io::Error::last_os_error()));
        }
        CloseHandle(token);
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut sid = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut sid) == 0 {
            return Err(storage(std::io::Error::last_os_error()));
        }
        let mut count = 0;
        while *sid.add(count) != 0 {
            count += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(sid, count));
        LocalFree(sid.cast());
        widestring::U16CString::from_str(format!("D:P(A;;GA;;;{text})")).map_err(storage)
    }
}

async fn read_frame(stream: &mut Stream) -> Result<Vec<u8>> {
    let size = stream.read_u32().await.map_err(storage)? as usize;
    if size > MAX_FRAME {
        return Err(invalid("IPC frame too large"));
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).await.map_err(storage)?;
    Ok(bytes)
}
async fn write_frame(stream: &mut Stream, data: &impl serde::Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(data).map_err(storage)?;
    if bytes.len() > MAX_FRAME {
        return Err(invalid("IPC frame too large"));
    }
    stream
        .write_u32(bytes.len() as u32)
        .await
        .map_err(storage)?;
    stream.write_all(&bytes).await.map_err(storage)?;
    stream.flush().await.map_err(storage)
}

pub async fn call(endpoint: &str, method: &str, params: Value) -> Result<Value> {
    let name = Path::new(endpoint)
        .to_fs_name::<GenericFilePath>()
        .map_err(storage)?;
    let mut stream = tokio::time::timeout(Duration::from_secs(10), Stream::connect(name))
        .await
        .map_err(|_| AppError::new(ErrorCode::RuntimeUnavailable, "IPC connection timed out"))?
        .map_err(|e| AppError::new(ErrorCode::RuntimeUnavailable, e.to_string()))?;
    write_frame(
        &mut stream,
        &RpcRequest {
            method: method.into(),
            params,
        },
    )
    .await?;
    let bytes = tokio::time::timeout(Duration::from_secs(15), read_frame(&mut stream))
        .await
        .map_err(|_| AppError::new(ErrorCode::RuntimeUnavailable, "runtime did not respond"))??;
    let response: RpcResponse = serde_json::from_slice(&bytes).map_err(storage)?;
    match (response.result, response.error) {
        (_, Some(error)) => Err(error),
        (Some(result), None) => Ok(result),
        _ => Err(storage("invalid IPC response")),
    }
}

pub async fn serve(runtime: Arc<Runtime>, listener: Listener) -> Result<()> {
    loop {
        tokio::select! {
            result=listener.accept()=>{let mut stream=result.map_err(storage)?;let runtime=runtime.clone();tokio::spawn(async move{
                let result:Result<Value>=async{let bytes=tokio::time::timeout(Duration::from_secs(10),read_frame(&mut stream)).await.map_err(|_|invalid("IPC request timed out"))??;let request:RpcRequest=serde_json::from_slice(&bytes).map_err(|e|invalid(e.to_string()))?;
                    let worker=runtime.clone();tokio::task::spawn_blocking(move||dispatch(&worker,&request.method,request.params)).await.map_err(storage)?}.await;
                let response=match result{Ok(result)=>RpcResponse{result:Some(result),error:None},Err(error)=>RpcResponse{result:None,error:Some(error)}};let _=write_frame(&mut stream,&response).await;
            });},
            _=tokio::time::sleep(Duration::from_millis(50))=>{if runtime.stopping.load(std::sync::atomic::Ordering::Acquire){break;}}
        }
    }
    Ok(())
}

pub fn dispatch(runtime: &Runtime, method: &str, params: Value) -> Result<Value> {
    match method {
        "handshake" => value(&runtime.info),
        "guide" => {
            let _: EmptyArgs = decode(params)?;
            value(GUIDE)
        }
        "status" | "doctor" => {
            let _: EmptyArgs = decode(params)?;
            value(runtime.status()?)
        }
        "list" => {
            let _: EmptyArgs = decode(params)?;
            value(runtime.library.list_documents()?)
        }
        "open" => value(runtime.open_view(decode::<OpenArgs>(params)?.document_id)?),
        "context" => value(runtime.context(decode::<ContextArgs>(params)?.view_id)?),
        "import" => {
            let args: ImportArgs = decode(params)?;
            let path = Path::new(&args.path);
            if !path.is_absolute() {
                return Err(invalid("import path must be absolute"));
            }
            let metadata = std::fs::metadata(path).map_err(storage)?;
            if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
                return Err(invalid("import requires a file of at most 32 MiB"));
            }
            let mut bytes = Vec::new();
            use std::io::Read;
            std::fs::File::open(path)
                .map_err(storage)?
                .take(32 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(storage)?;
            if bytes.len() > 32 * 1024 * 1024 {
                return Err(invalid("import exceeds 32 MiB"));
            }
            value(runtime.import(args, bytes)?)
        }
        "edit" => runtime.edit(decode(params)?, true),
        "check" => value(runtime.check(decode(params)?, false)?),
        "commit" => value(runtime.commit(decode(params)?)?),
        "history" => value(
            runtime
                .library
                .history(decode::<HistoryArgs>(params)?.document_id)?,
        ),
        "restore" => value(runtime.restore(decode(params)?)?),
        "export" => value(runtime.export(decode(params)?)?),
        "job" => {
            let args: JobArgs = decode(params)?;
            value(match args.action {
                JobAction::Get => runtime.library.get_job(args.job_id)?,
                JobAction::Cancel => runtime.cancel(args.job_id)?,
            })
        }
        "stop" => {
            runtime.stop()?;
            Ok(json!({"stopping":true}))
        }
        "integrate" => Err(AppError::unsupported(
            "host integration is not implemented; see P06",
        )),
        _ => Err(invalid("unknown runtime operation")),
    }
}

pub fn read_discovery(home: &Path) -> Result<Discovery> {
    let path = home.join("run/runtime.json");
    let meta = std::fs::symlink_metadata(&path)
        .map_err(|e| AppError::new(ErrorCode::RuntimeUnavailable, e.to_string()))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(storage("invalid runtime discovery file"));
    }
    let bytes = std::fs::read(path).map_err(storage)?;
    serde_json::from_slice(&bytes).map_err(|_| storage("invalid runtime discovery file"))
}

pub fn verify_handshake(home: &Path, info: &RuntimeInfo) -> Result<()> {
    if info.protocol_version != PROTOCOL_VERSION
        || info.schema_version != SCHEMA_VERSION
        || Path::new(&info.home) != home
    {
        return Err(AppError::new(
            ErrorCode::RuntimeIncompatible,
            "runtime home, protocol or schema mismatch",
        ));
    }
    Ok(())
}

pub async fn connect(home: &Path) -> Result<Discovery> {
    let discovery = read_discovery(home)?;
    let expected = endpoint(home)?;
    if Path::new(&discovery.endpoint) != expected {
        return Err(storage("discovery endpoint does not match library"));
    }
    let info: RuntimeInfo = decode(call(&discovery.endpoint, "handshake", json!({})).await?)?;
    verify_handshake(home, &info)?;
    if info.instance_id != discovery.runtime.instance_id {
        return Err(AppError::new(
            ErrorCode::RuntimeUnavailable,
            "runtime discovery changed; retry",
        ));
    }
    Ok(Discovery {
        runtime: info,
        endpoint: discovery.endpoint,
    })
}
