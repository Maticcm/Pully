/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        ink: "#19221c",
        canvas: "rgb(var(--color-canvas) / <alpha-value>)",
        surface: "rgb(var(--color-surface) / <alpha-value>)",
        lime: "rgb(var(--color-accent) / <alpha-value>)",
        accentForeground: "rgb(var(--color-accent-foreground) / <alpha-value>)",
      },
      fontFamily: { sans: ["var(--font-ui)"] },
      boxShadow: { float: "0 18px 60px rgba(25,34,28,.10)" },
    },
  },
  plugins: [],
};
