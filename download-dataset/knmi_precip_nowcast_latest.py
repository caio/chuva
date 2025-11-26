import logging
import os
import sys
import subprocess

from glob import glob

import requests

logging.basicConfig()
logger = logging.getLogger(__name__)
logger.setLevel(os.environ.get("LOG_LEVEL", logging.INFO))


class OpenDataAPI:
    def __init__(self, api_token: str):
        self.base_url = "https://api.dataplatform.knmi.nl/open-data/v1"
        self.headers = {"Authorization": api_token}

    def __get_data(self, url, params=None):
        return requests.get(url, headers=self.headers, params=params).json()

    def list_files(self, dataset_name: str, dataset_version: str, params: dict):
        return self.__get_data(
            f"{self.base_url}/datasets/{dataset_name}/versions/{dataset_version}/files",
            params=params,
        )

    def get_file_url(self, dataset_name: str, dataset_version: str, file_name: str):
        return self.__get_data(
            f"{self.base_url}/datasets/{dataset_name}/versions/{dataset_version}/files/{file_name}/url"
        )


def download_file_from_temporary_download_url(download_url, filename):
    try:
        with requests.get(download_url, stream=True) as r:
            r.raise_for_status()
            # Download to a different target then rename to the
            # actual so that there's never an invalid datafile
            # with a .h5 ext
            partial = f"{filename}.downloading"
            with open(partial, "wb") as f:
                for chunk in r.iter_content(chunk_size=8192):
                    f.write(chunk)
            os.rename(partial, filename)
    except Exception:
        logger.exception("Unable to download file using download URL")
        sys.exit(1)

    logger.info(f"Successfully downloaded dataset file to {filename}")


def get_api_key():
    dir = os.environ.get("CREDENTIALS_DIRECTORY")
    if dir is not None:
        cred = os.path.join(dir, "api")
        if not os.path.exists(cred):
            logger.error("systemd credentials for key 'api' not found")
            sys.exit(1)
        with open(cred, "r") as f:
            return f.read()

    tok = os.environ.get("API_TOKEN")
    if tok is not None:
        logger.warning("No systemd-creds info for process. Using $API_TOKEN")
        return tok

    logger.error("No credentials found")
    sys.exit(1)


def main():
    if len(sys.argv) != 2:
        logger.error(f"usage: {sys.argv[0]} <DOWNLOAD_DIR>")
        sys.exit(1)
    download_dir = sys.argv[1]

    api_key = get_api_key()
    dataset_name = "radar_forecast"
    dataset_version = "2.0"
    logger.info(f"Fetching latest file of {dataset_name} version {dataset_version}")

    api = OpenDataAPI(api_token=api_key)

    # sort the files in descending order and only retrieve the first file
    params = {"maxKeys": 1, "orderBy": "created", "sorting": "desc"}
    response = api.list_files(dataset_name, dataset_version, params)
    if "error" in response:
        logger.error(f"Unable to retrieve list of files: {response['error']}")
        sys.exit(1)

    latest_file = response["files"][0].get("filename")
    logger.info(f"Latest file is: {latest_file}")

    target = os.path.join(download_dir, latest_file)
    if os.path.exists(target):
        logger.info("Already downloaded. Nothing to do")
        sys.exit(0)

    # fetch the download url and download the file
    response = api.get_file_url(dataset_name, dataset_version, latest_file)
    download_file_from_temporary_download_url(response["temporaryDownloadUrl"], target)

    # data files by most-recent-first
    # lexy sort is enough due to the naming convention
    datasets = sorted(
        glob(os.path.join(download_dir, "RAD_NL25_RAC_FM_*.h5")), reverse=True
    )
    # keep a 2h window of datafiles (a new one every 5min)
    for file in datasets[24:]:
        logger.info(f"Deleting old dataset {file}")
        os.remove(file)

    logger.info("Killing moros process")
    res = subprocess.run(
        "/bin/sh -c 'kill -TERM $(pidof moros)'", shell=True, capture_output=True
    )
    if res.returncode != 0:
        logger.error("Failed to kill moros: %s", res.stderr)

    logger.info("Done")


if __name__ == "__main__":
    main()
