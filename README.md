# MeasureKit

A Python package for handling measurement units and conversions.

## Description

MeasureKit is a Python library designed to simplify working with measurement units, unit conversions, and dimensional analysis. It provides an intuitive interface for scientists, engineers, and developers working with physical quantities.

## Installation

You can install the package via pip:

```bash
pip install measurekit
```

Install it as a ipython kernel:

```bash
pip install ipykernel
python -m ipykernel install --user --name measurekit
```

## Usage

```python
import measurekit as mk

# Convert between units
meters = mk.convert(3.28, 'feet', 'meters')
print(f"3.28 feet = {meters} meters")

# Create a quantity with units
distance = mk.Quantity(5, 'km')
time = mk.Quantity(2, 'hours')

# Calculate speed
speed = distance / time
print(f"Speed: {speed}")  # Outputs with correct units (km/h)

# Convert to different units
speed_in_mph = speed.to('mph')
print(f"Speed in mph: {speed_in_mph}")
```

## Features

- Comprehensive unit conversion system
- Support for SI, imperial, and custom units
- Dimensional analysis to prevent invalid operations
- Unit-aware calculations
- Extensible unit definition system
- Automatic unit simplification and normalization

## Requirements

- Python 3.6 or higher

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Author

Irvin Torres ([irvinrx1996@hotmail.com](mailto:irvinrx1996@hotmail.com))
