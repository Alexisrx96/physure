"""Physical constants as Physure quantities."""

from physure import Q_

# CODATA 2018 values (most recent official ones as of general knowledge)
# Note: In the 2019 SI redefinition, c, h, e, k are exact.

c = Q_(299792458.0, "m/s")
h = Q_(6.62607015e-34, "J*s")
G = Q_(6.67430e-11, "m^3/kg/s^2", uncertainty=0.00015e-11)
k = Q_(1.380649e-23, "J/K")

__all__ = ["G", "c", "h", "k"]
