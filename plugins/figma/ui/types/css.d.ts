/**
 * Ambient declaration for the stylesheet import.
 *
 * The bundler turns the import of the stylesheet into a sibling stylesheet
 * next to the bundle. The compiler only has to accept the import.
 */
declare module "*.css";
