Consume the source and store all the samples in RAM using a `SamplesBuffer`.

# Panics
- Panics if the length of the buffer is larger than approximately 16 billion elements.
  This is because the calculation of the duration would overflow.
