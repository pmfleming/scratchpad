import urllib.request
import re
import os

fonts = ["Roboto", "Roboto Flex", "Roboto Mono", "Roboto Slab", "Roboto Serif"]
os.makedirs("fonts", exist_ok=True)

for font in fonts:
    family = font.replace(' ', '+')
    url = f"https://fonts.googleapis.com/css?family={family}:400"
    print(f"Fetching CSS for {font}...")
    
    # Old User-Agent to force TTF format (Android 4.1.1)
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0 (Linux; U; Android 4.1.1; en-gb; Build/KLP) AppleWebKit/534.30 (KHTML, like Gecko) Version/4.0 Safari/534.30'})
    try:
        with urllib.request.urlopen(req) as response:
            css = response.read().decode('utf-8')
            print(f"CSS:\n{css}")
            
            # Find the url(...) for the normal/400 font
            match = re.search(r"url\(([^)]+)\)", css)
            if match:
                ttf_url = match.group(1).strip("'\"")
                target_name = font.replace(" ", "") + "-Regular.ttf"
                
                print(f"Downloading {ttf_url} to {target_name}...")
                with urllib.request.urlopen(ttf_url) as ttf_resp:
                    with open(os.path.join("fonts", target_name), "wb") as f:
                        f.write(ttf_resp.read())
            else:
                print(f"Could not find TTF URL for {font} in CSS.")
    except Exception as e:
        print(f"Failed to download {font}: {e}")
