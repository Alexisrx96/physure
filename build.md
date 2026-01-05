# Build and Upload

To build and upload the package to PyPI, run the following commands:

```cmd
python setup.py sdist bdist_wheel
```

To check the package, run:

```cmd
twine check dist/*
```

To upload the package, run:

```cmd
twine upload dist/*
```
