import unittest
from check_distribution import validate

class BoundaryTests(unittest.TestCase):
    def test_private_cores_and_user_data_are_rejected(self):
        for path in ['core/policy.js','engine/src/main.rs','engine/worker/src/lib.rs','.private/report.txt','models/private.gguf','catalog.sqlite']:
            with self.subTest(path=path), self.assertRaises(ValueError): validate(path,b'harmless fixture')
    def test_public_client_paths_remain_allowed(self):
        for path in ['engine/companion/src/auth.rs','web-companion/app.js','Sources/PhotoFindryBrowserHost/App.swift']:
            validate(path,b'harmless fixture')
    def test_secrets_and_executable_payloads_are_rejected(self):
        for data in [b'-----BEGIN '+b'OPENSSH PRIVATE KEY-----',b'\x7fELFfixture']:
            with self.assertRaises(ValueError): validate('fixture.txt',data)
    def test_unpinned_or_privileged_workflows_are_rejected(self):
        for data in [b'uses: actions/checkout@main',b'on:\n  pull_request_target:',b'key: ${{ secrets.EXAMPLE }}']:
            with self.assertRaises(ValueError): validate('.github/workflows/check.yml',data)

if __name__ == '__main__': unittest.main()
