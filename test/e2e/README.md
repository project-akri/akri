# Running E2E Tests Locally

Reference the [Akri developer guide](https://docs.akri.sh/development/test-cases-workflow#run-the-tests-locally) for details on running locally.

## Displaying Output
By default, pytest captures output. If you always want to disable output capturing when running pytest through poetry, you can set the `PYTEST_ADDOPTS` environment variable:

```sh
PYTEST_ADDOPTS="-s" poetry run pytest -v | tee output.log
```

## Running a Specific Test
A specific test can be executed by specifying its file and name like so:
```
poetry run pytest  test_core.py::test_device_offline -vvv
```