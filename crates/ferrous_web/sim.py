import math
for z in range(48):
    for x in range(48):
        px = x * 0.6 - (48 * 0.5 * 0.6)
        pz = z * 0.6 - (48 * 0.5 * 0.6)
        py = math.sin(px * 0.25) * math.cos(pz * 0.25) * 2.5 + math.sin(px * 0.6) * 0.4
        if math.isnan(py) or abs(py) > 10.0:
            print("Anomaly at", x, z, py)
print("Done")
