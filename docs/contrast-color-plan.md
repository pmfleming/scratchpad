# Plan for High Contrast Text Color Selection

## Objective
Create a function that automatically selects an aesthetically pleasing and easily readable text color to be displayed on top of a given highlight color (which acts as the background). The selection will combine color theory principles with accessibility standards.

## Methodology
The approach merges two concepts to achieve both visual harmony and legibility:
1.  **Color Theory (Hue):** Finding a color that naturally contrasts or complements the background hue on the color wheel (e.g., complementary or split-complementary colors).
2.  **Luminance Contrast (WCAG):** Ensuring the difference in perceived lightness between the background and the text color meets WCAG 2 readability standards, preventing the colors from vibrating or clashing.

## Steps

1. **Accept the Highlight Color:**
   - The function will take the chosen highlight color as its input parameter (typically in an HSL or RGB format). This represents the background.

2. **Apply Color Theory to Determine Base Hue:**
   - Convert the highlight color to HSL (Hue, Saturation, Lightness) if it isn't already.
   - Calculate the **complementary hue** by rotating the original hue by 180 degrees on the color wheel. This provides the base for a color that theoretically contrasts well visually.
   - *(Optional)* Explore split-complementary hues (150 and 210 degrees) or triadic hues (120 and 240 degrees) if a less direct clash is desired.

3. **Adjust Lightness for Readability (Luminance Contrast):**
   - Pure complementary colors of the same lightness often vibrate and are hard to read (e.g., bright red on bright green).
   - Evaluate the perceived luminance (lightness) of the original highlight color.
   - If the highlight color is **light** (high luminance), adjust the lightness of the complementary text color to be very **dark**.
   - If the highlight color is **dark** (low luminance), adjust the lightness of the complementary text color to be very **light**.

4. **Verify with WCAG Contrast Ratios:**
   - Calculate the WCAG 2 contrast ratio between the highlight background and the newly adjusted complementary color using the `contrast` crate.
   - Ensure the ratio meets at least the minimum standard for text readability (usually 4.5:1 for normal text or 3.0:1 for large text).

5. **Determine Final Text Color:**
   - If the adjusted complementary color meets the WCAG contrast requirements, select it as the final text color.
   - **Fallback Mechanism:** If it is impossible to achieve a readable contrast ratio with the complementary hue (e.g., mid-tone backgrounds), fall back to pure White or pure Black, calculating which of the two offers the highest WCAG contrast ratio.

6. **Return the Result:**
   - Return the selected color, providing a result that is both theoretically harmonious and practically readable.
