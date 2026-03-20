#!/usr/bin/env python3
"""Example: Advanced progress tracking with progress bar"""

import sys
import time
import ztus


class ProgressTracker:
    """Advanced progress tracker with ETA calculation"""

    def __init__(self, total_bytes: int):
        self.total_bytes = total_bytes
        self.start_time = time.time()
        self.last_bytes = 0
        self.last_time = self.start_time

    def __call__(self, bytes_transferred: int, total_bytes: int) -> None:
        """Progress callback with ETA calculation

        Args:
            bytes_transferred: Total bytes transferred so far
            total_bytes: Total bytes to transfer
        """
        current_time = time.time()

        # Calculate progress percentage
        if total_bytes > 0:
            percent = (bytes_transferred / total_bytes) * 100

            # Calculate transfer speed (bytes/second)
            time_elapsed = current_time - self.last_time
            bytes_diff = bytes_transferred - self.last_bytes

            if time_elapsed > 0:
                speed = bytes_diff / time_elapsed
                speed_mb = speed / (1024 * 1024)

                # Calculate ETA
                bytes_remaining = total_bytes - bytes_transferred
                if speed > 0:
                    eta_seconds = bytes_remaining / speed
                    eta_str = f"{int(eta_seconds // 60)}m {int(eta_seconds % 60)}s"
                else:
                    eta_str = "Unknown"

                # Update progress bar
                bar_width = 40
                filled = int(bar_width * percent / 100)
                bar = "█" * filled + "░" * (bar_width - filled)

                print(
                    f"\r[{bar}] {percent:.1f}% | "
                    f"{speed_mb:.2f} MB/s | ETA: {eta_str}",
                    end="",
                    flush=True
                )

            # Update last values
            self.last_bytes = bytes_transferred
            self.last_time = current_time

    def finish(self):
        """Print final progress and newline"""
        total_time = time.time() - self.start_time
        avg_speed = self.total_bytes / total_time / (1024 * 1024)
        print(f"\nUpload complete! Average speed: {avg_speed:.2f} MB/s, Time: {total_time:.1f}s")


def main() -> None:
    # Create TUS client
    client = ztus.TusClient()

    # For demonstration, you would replace these with actual values
    file_path = "/path/to/large_file.bin"
    upload_url = "https://tus.server.tld/files"

    # Get file size for progress tracker
    import os
    file_size = os.path.getsize(file_path) if os.path.exists(file_path) else 0

    # Create progress tracker
    tracker = ProgressTracker(file_size)

    # Upload with progress tracking
    try:
        client.upload(
            file_path,
            upload_url,
            progress_callback=tracker
        )
        tracker.finish()
    except Exception as e:
        print(f"\nUpload failed: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
