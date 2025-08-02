import os
import requests
import time
import argparse

API_URL = "https://router.huggingface.co/v1/chat/completions"

headers = {
    "Authorization": f"Bearer {os.environ['HF_TOKEN']}",
}


def query(payload):
    response = requests.post(API_URL, headers=headers, json=payload)
    return response.json()


def read_file_content(file_path):
    try:
        with open(file_path, "r", encoding="utf-8") as file:
            return file.read()
    except IOError as e:
        print(f"Error reading file {file_path}: {e}")
        return None


def process_api_call(args):
    model = args.model
    prompt_file = args.prompt
    input_file = args.input
    output_file = args.output
    max_tokens = args.max_tokens

    prompt_content = read_file_content(prompt_file)
    if not prompt_content:
        print("Prompt content is empty")
        return

    messages = []

    if input_file:
        input_content = read_file_content(input_file)
        if not input_content:
            print("Input content is empty")
            return

        messages = [{"role": "user", "content": f"{prompt_content}\n{input_content}"}]

    payload = {"messages": messages, "model": model, "max_tokens": max_tokens}

    response = query(payload)

    if "choices" in response and response["choices"]:
        api_response_content = response["choices"][0]["message"]["content"]

        if output_file:
            try:
                with open(output_file, "w", encoding="utf-8") as file:
                    file.write(api_response_content)
                print(f"API response saved to {output_file}")
            except IOError as e:
                print(f"Error writing to file {output_file}: {e}")
        else:
            print("API Response:")
            print(api_response_content)
    else:
        print("Error or empty response from API:", response)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Query a Hugging Face model with a prompt and input file, writing the response to an output file."
    )

    parser.add_argument(
        "--model",
        type=str,
        required=True,
        help="The name of the Hugging Face model to use.",
    )
    parser.add_argument(
        "--max_tokens",
        type=str,
        required=True,
        help="The maximal total tokens the model should produce.",
    )
    parser.add_argument(
        "--prompt",
        type=str,
        required=True,
        help="The file path containing the main prompt text.",
    )
    parser.add_argument(
        "--input",
        type=str,
        default=None,
        help="The file path containing input data to include in the query (optional).",
    )
    parser.add_argument(
        "--output",
        type=str,
        default=None,
        help="The file path to write the API response to. If not provided, the response is printed to the console (optional).",
    )

    args = parser.parse_args()

    s0 = time.time()
    process_api_call(args)
    print("Time elapsed:", time.time() - s0)
