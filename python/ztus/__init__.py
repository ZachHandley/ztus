"""ztus - Python bindings for the Rust TUS protocol client

This package provides both synchronous and asynchronous APIs for resumable
file uploads using the TUS protocol, with support for progress callbacks.
"""

# The actual implementation is in Rust, imported via PyO3
# This file exists to make this a valid Python package

__version__ = "1.0.16"  # Replaced by PKG_VERSION in CI/CD
__all__ = ["TusClient"]

# Re-export the Rust-compiled classes
try:
    from .ztus import TusClient  # type: ignore
except ImportError:
    # Development mode: may not be compiled yet
    pass
