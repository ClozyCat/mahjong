import os
import requests
import time

# 指定的 API 端点
API_URL = "https://tts.060889.xyz/v1/audio/speech"

# README中提供的所有男女声音色
VOICES = [
    # 女声
    "zh-CN-XiaoxiaoNeural", "zh-CN-XiaoyiNeural", "zh-CN-XiaochenNeural",
    "zh-CN-XiaohanNeural", "zh-CN-XiaomengNeural", "zh-CN-XiaomoNeural",
    "zh-CN-XiaoqiuNeural", "zh-CN-XiaoruiNeural", "zh-CN-XiaoshuangNeural",
    "zh-CN-XiaoxuanNeural", "zh-CN-XiaoyanNeural", "zh-CN-XiaoyouNeural",
    "zh-CN-XiaozhenNeural",
    # 男声
    "zh-CN-YunxiNeural", "zh-CN-YunyangNeural", "zh-CN-YunjianNeural",
    "zh-CN-YunfengNeural", "zh-CN-YunhaoNeural", "zh-CN-YunxiaNeural",
    "zh-CN-YunyeNeural", "zh-CN-YunzeNeural"
]

# 生成麻将词汇和对应的拼音文件名映射
numbers_zh = ["一", "二", "三", "四", "五", "六", "七", "八", "九"]
numbers_py = ["yi", "er", "san", "si", "wu", "liu", "qi", "ba", "jiu"]

mahjong_terms = []

# 生成 1-9 条
for z, p in zip(numbers_zh, numbers_py):
    mahjong_terms.append((f"{z}条", f"{p}_tiao"))
    
# 生成 1-9 万
for z, p in zip(numbers_zh, numbers_py):
    mahjong_terms.append((f"{z}万", f"{p}_wan"))
    
# 生成 1-9 筒
for z, p in zip(numbers_zh, numbers_py):
    mahjong_terms.append((f"{z}筒", f"{p}_tong"))

# 添加特殊动作牌和字牌（东南西北中发白）
mahjong_terms.extend([
    ("杠", "gang"),
    ("碰", "peng"),
    ("吃", "chi"),
    ("胡", "hu"),   # 注意“和”在这里读 hu
    ("东", "dong"),
    ("南", "nan"),
    ("西", "xi"),
    ("北", "bei"),
    ("中", "zhong"),
    ("发", "fa"),
    ("白", "bai")
])

def main():
    # 创建基础文件夹
    base_dir = "Mahjong_Voices"
    if not os.path.exists(base_dir):
        os.makedirs(base_dir)

    total_voices = len(VOICES)
    
    for i, voice in enumerate(VOICES, 1):
        # 为每种声音创建一个独立文件夹
        voice_dir = os.path.join(base_dir, voice)
        if not os.path.exists(voice_dir):
            os.makedirs(voice_dir)
            
        print(f"\n[{i}/{total_voices}] 正在生成声音: {voice}")
        
        for text, pinyin in mahjong_terms:
            filepath = os.path.join(voice_dir, f"{pinyin}.mp3")
            
            # 如果文件已存在则跳过，方便中断后继续
            if os.path.exists(filepath):
                print(f"  [-] 已存在，跳过: {pinyin}.mp3")
                continue

            # 构造请求数据，指定风格为 assistant
            payload = {
                "input": text,
                "voice": voice,
                "speed": 1.0,
                "pitch": "0",
                "style": "assistant"
            }

            try:
                response = requests.post(
                    API_URL, 
                    json=payload,
                    headers={"Content-Type": "application/json"}
                )
                response.raise_for_status() # 检查请求是否成功

                # 保存音频文件
                with open(filepath, "wb") as f:
                    f.write(response.content)
                print(f"  [+] 成功生成: {pinyin}.mp3 ({text})")
                
                # 稍微休眠，避免请求过快导致被服务器阻截
                time.sleep(0.5)
                
            except Exception as e:
                print(f"  [x] 生成失败 {pinyin}.mp3: {e}")

    print("\n🎉 所有音效生成完毕！文件保存在当前目录的 Mahjong_Voices 文件夹下。")

if __name__ == "__main__":
    main()