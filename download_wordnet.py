import os
import urllib.request
import zipfile

# URL for the 2024 Open English WordNet WNDB ZIP
url = "https://en-word.net/static/english-wordnet-2024.zip"

# Where to save and where to extract
dest_zip = "open_english_wordnet_2024.zip"
extract_dir = "open_english_wordnet_2024"

def download(url: str, dest: str):
    print(f"Downloading {url} …")
    urllib.request.urlretrieve(url, dest)
    print(f"Saved to {dest}")

def unpack(zip_path: str, out_dir: str):
    print(f"Extracting {zip_path} → {out_dir} …")
    with zipfile.ZipFile(zip_path, 'r') as z:
        z.extractall(out_dir)
    print("Extraction complete.")

if __name__ == "__main__":
    # Download the file
    if not os.path.isfile(dest_zip):
        download(url, dest_zip)
    else:
        print(f"{dest_zip} already exists, skipping download.")

    # Unzip
    if not os.path.isdir(extract_dir):
        os.makedirs(extract_dir, exist_ok=True)
        unpack(dest_zip, extract_dir)
    else:
        print(f"{extract_dir} already exists, skipping extraction.")

    print("Done. The WNDB files should now be in:", extract_dir)
