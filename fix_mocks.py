import os
import glob

def fix_mocks():
    files = glob.glob('contracts/shipment/src/**/*.rs', recursive=True)
    for f in files:
        with open(f, 'r') as fp:
            content = fp.read()
            
        if "struct MockToken" in content and "pub fn decimals" not in content and "trait MockToken" not in content:
            # find impl MockToken {
            # and append pub fn decimals(_env: Env) -> u32 { 7 }
            if "impl MockToken {" in content:
                content = content.replace("impl MockToken {", "impl MockToken {\n    pub fn decimals(_env: soroban_sdk::Env) -> u32 { 7 }\n")
                with open(f, 'w') as fp:
                    fp.write(content)
                print(f"Fixed {f}")
            elif "impl MockTokenConsistency {" in content:
                content = content.replace("impl MockTokenConsistency {", "impl MockTokenConsistency {\n    pub fn decimals(_env: soroban_sdk::Env) -> u32 { 7 }\n")
                with open(f, 'w') as fp:
                    fp.write(content)
                print(f"Fixed {f} (Consistency)")

if __name__ == '__main__':
    fix_mocks()
