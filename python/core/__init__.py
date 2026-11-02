"""内部实现包。用户只运行上一级的三个 main 文件。"""

from .runner import run_correctness, run_timing, run_pipeline

__all__ = ["run_correctness", "run_timing", "run_pipeline"]
