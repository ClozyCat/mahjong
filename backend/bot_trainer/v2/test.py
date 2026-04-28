import os
import sys

# 强制将当前目录和 site-packages 添加到 DLL 搜索路径（针对 Python 3.8+ 的特性）
import site
for p in site.getsitepackages():
    os.add_dll_directory(p)

try:
    import torch
    print("PyTorch imported successfully.")
    print(f"PyTorch version: {torch.__version__}")
    
    # 尝试初始化 CUDA/HIP
    is_avail = torch.cuda.is_available()
    print(f"Is CUDA/HIP available? {is_avail}")
    
    if is_avail:
        print(f"Device name: {torch.cuda.get_device_name(0)}")
except Exception as e:
    print(f"Caught an exception: {e}")