import os
import requests
import argparse

# Base URL for the Hugging Face Inference API
API_URL = "https://router.huggingface.co/v1/chat/completions"

# Headers for the API request, including the authorization token
# It's assumed that the 'HF_TOKEN' environment variable is set.
# Example: export HF_TOKEN="hf_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
headers = {
    "Authorization": f"Bearer {os.environ['HF_TOKEN']}",
}

def query(payload):
    response = requests.post(API_URL, headers=headers, json=payload)
    return response.json()

def read_file_content(file_path):
    try:
        with open(file_path, 'r', encoding='utf-8') as file:
            return file.read()
    except IOError as e:
        print(f"Error reading file {file_path}: {e}")
        return None

def process_api_call(model, prompt_file, input_file, output_file):
    # Read the main prompt content from the specified file
    prompt_content = read_file_content(prompt_file)
    if not prompt_content:
        return
    
    messages = []

    # Append content from the single input file, if provided
    if input_file:
        input_content = read_file_content(input_file)
        messages = [
            {"role": "user", "content": f'{prompt_content}\n{input_content}'}
        ]

    # Construct the final payload for the API
    payload = {
        "messages": messages,
        "model": model
    }

    # Make the API call
    response = query(payload)

    # Process the API response
    if "choices" in response and response["choices"]:
        api_response_content = response["choices"][0]["message"]["content"]
        
        # If an output file is specified, write the response to it
        if output_file:
            try:
                with open(output_file, 'w', encoding='utf-8') as file:
                    file.write(api_response_content)
                print(f"API response saved to {output_file}")
            except IOError as e:
                print(f"Error writing to file {output_file}: {e}")
        else:
            # If no output file, print the response to the console
            print("API Response:")
            print(api_response_content)
    else:
        print("Error or empty response from API:", response)

# Main execution block
if __name__ == "__main__":
    # Set up argument parser
    parser = argparse.ArgumentParser(description="Query a Hugging Face model with a prompt and input file, writing the response to an output file.")
    
    # Define the command-line arguments
    parser.add_argument(
        "--model", 
        type=str, 
        required=True, 
        help="The name of the Hugging Face model to use."
    )
    parser.add_argument(
        "--prompt", 
        type=str, 
        required=True, 
        help="The file path containing the main prompt text."
    )
    parser.add_argument(
        "--input", 
        type=str, 
        default=None, 
        help="The file path containing input data to include in the query (optional)."
    )
    parser.add_argument(
        "--output", 
        type=str, 
        default=None, 
        help="The file path to write the API response to. If not provided, the response is printed to the console (optional)."
    )

    # Parse the arguments from the command line
    args = parser.parse_args()

    # Call the main function with the parsed arguments
    process_api_call(args.model, args.prompt, args.input, args.output)
