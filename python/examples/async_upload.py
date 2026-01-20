#!/usr/bin/env python3
"""Example: Asynchronous file upload with progress callback"""

import asyncio
import ztus


def progress_callback(bytes_transferred: int, total_bytes: int) -> None:
    """Progress callback function

    Args:
        bytes_transferred: Total bytes transferred so far
        total_bytes: Total bytes to transfer
    """
    if total_bytes > 0:
        percent = (bytes_transferred / total_bytes) * 100
        print(f"Progress: {bytes_transferred}/{total_bytes} bytes ({percent:.1f}%)")
    else:
        print(f"Progress: {bytes_transferred} bytes transferred")


async def main() -> None:
    # Create TUS client
    client = ztus.TusClient()

    # Upload file asynchronously with progress callback
    await client.upload_async(
        "/path/to/file.txt",
        "https://tus.server.tld/files",
        progress_callback=progress_callback
    )

    print("Upload complete!")


if __name__ == "__main__":
    asyncio.run(main())
