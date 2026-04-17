import os
import urllib.request

FONT_URLS = {
    "NotoSansDisplay-Regular.ttf": "https://raw.githubusercontent.com/notofonts/noto-fonts/main/hinted/ttf/NotoSansDisplay/NotoSansDisplay-Regular.ttf",
    "NotoSans-VF.ttf": "https://raw.githubusercontent.com/notofonts/noto-fonts/main/unhinted/variable-ttf/NotoSans-VF.ttf",
    "NotoSansMono-Regular.ttf": "https://raw.githubusercontent.com/notofonts/noto-fonts/main/hinted/ttf/NotoSansMono/NotoSansMono-Regular.ttf",
    "NotoSerifDisplay-Regular.ttf": "https://raw.githubusercontent.com/notofonts/noto-fonts/main/hinted/ttf/NotoSerifDisplay/NotoSerifDisplay-Regular.ttf",
    "NotoSansCJKjp-Regular.otf": "https://raw.githubusercontent.com/notofonts/noto-cjk/main/Sans/OTF/Japanese/NotoSansCJKjp-Regular.otf",
    "NotoSansCJKkr-Regular.otf": "https://raw.githubusercontent.com/notofonts/noto-cjk/main/Sans/OTF/Korean/NotoSansCJKkr-Regular.otf",
    "NotoSansCJKsc-Regular.otf": "https://raw.githubusercontent.com/notofonts/noto-cjk/main/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf",
    "NotoSansCJKtc-Regular.otf": "https://raw.githubusercontent.com/notofonts/noto-cjk/main/Sans/OTF/TraditionalChinese/NotoSansCJKtc-Regular.otf",
}

os.makedirs("fonts", exist_ok=True)

for file_name, url in FONT_URLS.items():
    print(f"Downloading {file_name}...")
    target_path = os.path.join("fonts", file_name)
    try:
        with urllib.request.urlopen(url) as response:
            with open(target_path, "wb") as font_file:
                font_file.write(response.read())
    except Exception as error:
        print(f"Failed to download {file_name}: {error}")
