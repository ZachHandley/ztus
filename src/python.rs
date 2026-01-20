//! Python bindings for ztus using PyO3
//!
//! This module provides both synchronous and asynchronous Python APIs for TUS operations.

#[cfg(feature = "python")]
use crate::client::TusClient;
#[cfg(feature = "python")]
use crate::error::ZtusError;
#[cfg(feature = "python")]
use crate::progress::ProgressReporter;
#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::PyAny;
#[cfg(feature = "python")]
use std::path::PathBuf;
#[cfg(feature = "python")]
use std::sync::Arc;

#[cfg(feature = "python")]
/// Python callback progress reporter
struct PythonProgressCallback {
    callback: Py<PyAny>,
}

#[cfg(feature = "python")]
impl ProgressReporter for PythonProgressCallback {
    fn report_progress(&self, bytes_transferred: u64, total_bytes: u64) -> crate::error::Result<()> {
        Python::with_gil(|py| {
            self.callback
                .call(py, (bytes_transferred, total_bytes), None)
                .map(|_| ())
                .map_err(|e| ZtusError::ConfigError(e.to_string()))
        })
    }

    fn set_total(&self, _total_bytes: u64) {
        // No-op for Python callbacks
    }

    fn finish(&self, _message: &str) {
        // No-op for Python callbacks
    }
}

#[cfg(feature = "python")]
/// Helper to convert Arc<dyn ProgressReporter> to Box<dyn ProgressReporter>
fn arc_to_box(progress: Arc<dyn ProgressReporter>) -> Box<dyn ProgressReporter> {
    // Create a wrapper struct that holds the Arc
    struct ArcWrapper {
        inner: Arc<dyn ProgressReporter>,
    }

    impl ProgressReporter for ArcWrapper {
        fn report_progress(&self, bytes_transferred: u64, total_bytes: u64) -> crate::error::Result<()> {
            self.inner.report_progress(bytes_transferred, total_bytes)
        }

        fn set_total(&self, total_bytes: u64) {
            self.inner.set_total(total_bytes)
        }

        fn finish(&self, message: &str) {
            self.inner.finish(message)
        }
    }

    Box::new(ArcWrapper { inner: progress })
}

#[cfg(feature = "python")]
/// TUS client for resumable uploads/downloads
#[pyclass(name = "TusClient")]
pub struct PyTusClient {
    inner: TusClient,
}

#[cfg(feature = "python")]
#[pymethods]
impl PyTusClient {
    /// Create a new TUS client with default configuration
    #[new]
    fn new() -> PyResult<Self> {
        let inner = TusClient::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Upload a file synchronously with optional progress callback
    ///
    /// Args:
    ///     file_path: Path to file to upload
    ///     upload_url: TUS server upload URL
    ///     progress_callback: Optional callback function(bytes_transferred, total_bytes)
    #[pyo3(signature = (file_path, upload_url, progress_callback=None))]
    fn upload(
        &self,
        file_path: &str,
        upload_url: &str,
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<()> {
        let file_path = PathBuf::from(file_path);
        let upload_config = self.inner.upload_config().clone();
        let state_dir = self.inner.state_dir().to_path_buf();

        let progress: Box<dyn ProgressReporter> = if let Some(callback) = progress_callback {
            Box::new(PythonProgressCallback { callback })
        } else {
            Box::new(crate::progress::NoOpProgress)
        };

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        rt.block_on(async {
            let manager = crate::upload::UploadManager::with_progress(
                upload_url.to_string(),
                upload_config,
                state_dir,
                progress,
            )
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            manager
                .upload_file(&file_path)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
        })
    }

    /// Upload a file asynchronously with optional progress callback
    ///
    /// Args:
    ///     file_path: Path to file to upload
    ///     upload_url: TUS server upload URL
    ///     progress_callback: Optional callback function(bytes_transferred, total_bytes)
    ///
    /// Returns:
    ///     Coroutine that completes when upload finishes
    #[pyo3(signature = (file_path, upload_url, progress_callback=None))]
    fn upload_async<'a>(
        &self,
        py: Python<'a>,
        file_path: &str,
        upload_url: &str,
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<&'a PyAny> {
        let file_path = PathBuf::from(file_path);
        let upload_url = upload_url.to_string();
        let upload_config = self.inner.upload_config().clone();
        let state_dir = self.inner.state_dir().to_path_buf();

        let progress = if let Some(callback) = progress_callback {
            Arc::new(PythonProgressCallback { callback }) as Arc<dyn ProgressReporter>
        } else {
            Arc::new(crate::progress::NoOpProgress) as Arc<dyn ProgressReporter>
        };

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let manager = crate::upload::UploadManager::with_progress(
                upload_url,
                upload_config,
                state_dir,
                arc_to_box(progress),
            )
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            manager
                .upload_file(&file_path)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            Ok(Python::with_gil(|py| py.None()))
        })
    }

    /// Resume an incomplete upload synchronously
    ///
    /// Args:
    ///     file_path: Path to file to upload
    ///     upload_url: TUS server upload URL
    ///     progress_callback: Optional callback function(bytes_transferred, total_bytes)
    #[pyo3(signature = (file_path, upload_url, progress_callback=None))]
    fn resume(
        &self,
        file_path: &str,
        upload_url: &str,
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<()> {
        let file_path = PathBuf::from(file_path);
        let upload_config = self.inner.upload_config().clone();
        let state_dir = self.inner.state_dir().to_path_buf();

        let progress: Box<dyn ProgressReporter> = if let Some(callback) = progress_callback {
            Box::new(PythonProgressCallback { callback })
        } else {
            Box::new(crate::progress::NoOpProgress)
        };

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        rt.block_on(async {
            let manager = crate::upload::UploadManager::with_progress(
                upload_url.to_string(),
                upload_config,
                state_dir,
                progress,
            )
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            manager
                .resume_upload(&file_path)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
        })
    }

    /// Resume an incomplete upload asynchronously
    #[pyo3(signature = (file_path, upload_url, progress_callback=None))]
    fn resume_async<'a>(
        &self,
        py: Python<'a>,
        file_path: &str,
        upload_url: &str,
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<&'a PyAny> {
        let file_path = PathBuf::from(file_path);
        let upload_url = upload_url.to_string();
        let upload_config = self.inner.upload_config().clone();
        let state_dir = self.inner.state_dir().to_path_buf();

        let progress = if let Some(callback) = progress_callback {
            Arc::new(PythonProgressCallback { callback }) as Arc<dyn ProgressReporter>
        } else {
            Arc::new(crate::progress::NoOpProgress) as Arc<dyn ProgressReporter>
        };

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let manager = crate::upload::UploadManager::with_progress(
                upload_url,
                upload_config,
                state_dir,
                arc_to_box(progress),
            )
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            manager
                .resume_upload(&file_path)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            Ok(Python::with_gil(|py| py.None()))
        })
    }

    /// Download a file synchronously
    ///
    /// Args:
    ///     url: URL to download from
    ///     output_path: Where to save the file
    ///     chunk_size: Optional chunk size in bytes (default 5MB)
    ///     progress_callback: Optional callback function(bytes_transferred, total_bytes)
    #[pyo3(signature = (url, output_path, chunk_size=None, progress_callback=None))]
    fn download(
        &self,
        url: &str,
        output_path: &str,
        chunk_size: Option<usize>,
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<()> {
        let output_path = PathBuf::from(output_path);
        let chunk_size = chunk_size.unwrap_or(5 * 1024 * 1024); // 5 MB default

        let progress: Box<dyn ProgressReporter> = if let Some(callback) = progress_callback {
            Box::new(PythonProgressCallback { callback })
        } else {
            Box::new(crate::progress::NoOpProgress)
        };

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        rt.block_on(async {
            let manager = crate::download::DownloadManager::with_progress(chunk_size, progress).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            manager
                .download_file(&url, &output_path)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
        })
    }

    /// Download a file asynchronously
    #[pyo3(signature = (url, output_path, chunk_size=None, progress_callback=None))]
    fn download_async<'a>(
        &self,
        py: Python<'a>,
        url: &str,
        output_path: &str,
        chunk_size: Option<usize>,
        progress_callback: Option<Py<PyAny>>,
    ) -> PyResult<&'a PyAny> {
        let url = url.to_string();
        let output_path = PathBuf::from(output_path);
        let chunk_size = chunk_size.unwrap_or(5 * 1024 * 1024);

        let progress = if let Some(callback) = progress_callback {
            Arc::new(PythonProgressCallback { callback }) as Arc<dyn ProgressReporter>
        } else {
            Arc::new(crate::progress::NoOpProgress) as Arc<dyn ProgressReporter>
        };

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let manager = crate::download::DownloadManager::with_progress(
                chunk_size,
                arc_to_box(progress),
            )
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            manager
                .download_file(&url, &output_path)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            Ok(Python::with_gil(|py| py.None()))
        })
    }

    /// List incomplete uploads
    fn list_incomplete(&self) -> PyResult<Vec<String>> {
        self.inner
            .list_incomplete()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Clean up old incomplete uploads
    ///
    /// Args:
    ///     days: Number of days after which to clean up
    ///
    /// Returns:
    ///     Number of uploads cleaned up
    fn cleanup(&self, days: i64) -> PyResult<usize> {
        self.inner
            .cleanup(days)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }
}

#[cfg(feature = "python")]
#[pymodule]
fn ztus(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyTusClient>()?;
    Ok(())
}
